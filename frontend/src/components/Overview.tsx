import React, { useEffect, useRef, useState } from 'react';
import {
  ApiStatusResponse,
  ProviderRuntimeDetail,
  LiveStreamEvent,
} from '../types';
import {
  Activity,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Zap,
  Clock,
  RotateCcw,
  ShieldCheck,
  AlertOctagon,
  ArrowRight,
  ChevronRight,
  Filter,
  CircuitBoard,
  Workflow,
  Bot,
  Sliders,
  Save,
} from 'lucide-react';
import { resetProviderCircuit } from '../api';
import {
  Card,
  MetricCard,
  Button,
  Badge,
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
  cn,
} from '../ui';
import { LiveAgentDeck, ActiveStreamState } from './LiveAgentDeck';
import { AgentSetupHero } from './AgentSetupHero';

interface OverviewProps {
  status: ApiStatusResponse | null;
  onRefresh: () => void;
  liveEvent?: LiveStreamEvent | null;
  recentStreams?: Array<{ event: LiveStreamEvent; timestamp_ms: number }> | null;
  onNavigateToProviders?: () => void;
  isCloudOrProtected?: boolean;
}

interface TargetEvaluation {
  status: 'running' | 'busy' | 'primary' | 'standby' | 'blocked';
  reason: string;
  inFlight: number;
  circuit: string;
  /** Remaining cooldown/timeout seconds to render in a dedicated high-contrast timer box */
  cooldownSecs?: number;
  /** Which other virtual model(s) are currently occupying this provider when status === 'busy' */
  busyByVirtualModel?: string;
}

interface RoutingPolicyConfig {
  stream_header_timeout_secs: number;
  nonstream_header_timeout_secs: number;
  fault_window_secs: number;
  overload_base_cooldown_secs: number;
  overload_max_cooldown_secs: number;
  circuit_failure_threshold: number;
  circuit_base_cooldown_secs: number;
  circuit_max_cooldown_secs: number;
  rate_limit_default_cooldown_secs: number;
  decay_interval_secs: number;
}

const DEFAULT_ROUTING_POLICY: RoutingPolicyConfig = {
  stream_header_timeout_secs: 22,
  nonstream_header_timeout_secs: 300,
  fault_window_secs: 180,
  overload_base_cooldown_secs: 15,
  overload_max_cooldown_secs: 90,
  circuit_failure_threshold: 3,
  circuit_base_cooldown_secs: 30,
  circuit_max_cooldown_secs: 120,
  rate_limit_default_cooldown_secs: 10,
  decay_interval_secs: 180,
};

function evaluateTarget(
  providerName: string,
  _modelName: string,
  providers: ProviderRuntimeDetail[],
  isFirstHealthyFound: { current: boolean },
  isOccupiedByOtherVm: boolean = false
): TargetEvaluation {
  const p = providers.find((prov) => prov.name === providerName);
  if (!p || !p.enabled) {
    return {
      status: 'blocked',
      reason: 'Provider Disabled',
      inFlight: 0,
      circuit: 'Disabled',
    };
  }

  const isTripped = p.circuit.startsWith('Open');
  const inFlight = p.in_flight || 0;

  // Severed upstream (model removed/EOL): auto-disabled providers must render
  // as blocked — pre-gap they were indistinguishable from healthy standby.
  if (p.auto_disabled_reason) {
    return {
      status: 'blocked',
      reason: `Auto-disabled: ${p.auto_disabled_reason}`,
      inFlight,
      circuit: p.circuit,
    };
  }

  if (isTripped) {
    const cooldownMatch = p.circuit.match(/(\d+)s/);
    const secs = cooldownMatch ? parseInt(cooldownMatch[1], 10) : undefined;
    return {
      status: 'blocked',
      reason: `Circuit Open (${p.failures} fails)`,
      cooldownSecs: secs,
      inFlight,
      circuit: p.circuit,
    };
  }

  if (p.rate_limited) {
    return {
      status: 'blocked',
      reason: p.failures > 0 ? `Cooldown (${p.failures} fails)` : '429 Rate-Limited',
      cooldownSecs: p.rate_limited_until_secs ?? undefined,
      inFlight,
      circuit: p.circuit,
    };
  }

  if (p.is_rpm_capped) {
    return {
      status: 'blocked',
      reason: `RPM Capped (${p.current_rpm}/${p.max_rpm})`,
      inFlight,
      circuit: p.circuit,
    };
  }

  if (p.is_burst_limited) {
    return {
      status: 'blocked',
      reason: `Burst Spacing (${p.min_interval_ms}ms)`,
      cooldownSecs: Math.max(1, Math.ceil((p.min_interval_ms || 1000) / 1000)),
      inFlight,
      circuit: p.circuit,
    };
  }

  if (isOccupiedByOtherVm) {
    return {
      status: 'busy',
      reason: inFlight > 0 ? `Busy (${inFlight} req in-flight)` : 'Provider Busy',
      inFlight,
      circuit: p.circuit,
    };
  }

  if (!isFirstHealthyFound.current) {
    isFirstHealthyFound.current = true;
    return {
      status: 'primary',
      reason: inFlight > 0 ? `Primary Route (${inFlight} req active)` : 'Primary Healthy Route',
      inFlight,
      circuit: p.circuit,
    };
  }

  return {
    status: 'standby',
    reason: inFlight > 0 ? `Standby Cascade (${inFlight} req active)` : 'Standby Cascade Target',
    inFlight,
    circuit: p.circuit,
  };
}

const STORAGE_KEY = 'wirellm_live_streams_v2';
const FIVE_MINUTES_MS = 5 * 60 * 1000;

export const Overview: React.FC<OverviewProps> = ({
  status,
  onRefresh,
  liveEvent,
  recentStreams,
  onNavigateToProviders,
  isCloudOrProtected = false,
}) => {
  const [selectedVmTab, setSelectedVmTab] = useState<string>('all');
  const [filterMode, setFilterMode] = useState<'all' | 'active_blocked'>('all');
  const [showAgentSetup, setShowAgentSetup] = useState<boolean>(false);
  const [showPolicyPanel, setShowPolicyPanel] = useState<boolean>(false);
  const [routingPolicy, setRoutingPolicy] = useState<RoutingPolicyConfig>(DEFAULT_ROUTING_POLICY);
  const [savingPolicy, setSavingPolicy] = useState<boolean>(false);
  const [policySavedMsg, setPolicySavedMsg] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/routing-policy')
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        if (data && typeof data.stream_header_timeout_secs === 'number') {
          setRoutingPolicy(data);
        }
      })
      .catch(() => {});
  }, []);

  const handleSaveRoutingPolicy = async () => {
    setSavingPolicy(true);
    setPolicySavedMsg(null);
    try {
      const res = await fetch('/api/routing-policy', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(routingPolicy),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      // The API returns the (clamped) policy directly — mirror it back so the
      // form shows the server-normalized values, not the raw typed ones.
      const data = await res.json();
      if (data && typeof data.stream_header_timeout_secs === 'number') {
        setRoutingPolicy(data);
      }
      setPolicySavedMsg('Saved & applied live!');
      setTimeout(() => setPolicySavedMsg(null), 3500);
      onRefresh();
    } catch (err: any) {
      setPolicySavedMsg(`Error: ${err.message || 'Failed to save'}`);
    } finally {
      setSavingPolicy(false);
    }
  };

  // Initialize from localStorage (pruning stale streams older than 3 minutes, and marking >12s inactive streams as done)
  const [activeStreams, setActiveStreams] = useState<Record<string, ActiveStreamState>>(() => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) {
        const parsed = JSON.parse(saved);
        const now = Date.now();
        const filtered: Record<string, ActiveStreamState> = {};
        for (const [k, v] of Object.entries(parsed as Record<string, ActiveStreamState>)) {
          const age = now - (v.lastUpdated || 0);
          if (age <= 180_000) { // Keep only within 3 minutes
            filtered[k] = {
              ...v,
              isDone: v.isDone || age > 180_000,
            };
          }
        }
        return filtered;
      }
    } catch {
      // ignore
    }
    return {};
  });

  // Persist to localStorage on change
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(activeStreams));
    } catch {
      // ignore
    }
  }, [activeStreams]);

  // Latest status snapshot in a ref: the 1s status tick must not recreate the
  // long-lived pruner interval below (a `[status]` dep would restart it every
  // second, so the 2s tick would never fire and idle streams would never expire).
  const statusRef = useRef<ApiStatusResponse | null>(status);
  useEffect(() => {
    statusRef.current = status;
  }, [status]);

  // Periodic pruner: mark streams inactive for >10s as done (unless provider still has in_flight > 0), and remove streams older than 3 minutes
  useEffect(() => {
    const timer = setInterval(() => {
      setActiveStreams((prev) => {
        const now = Date.now();
        let changed = false;
        const next: Record<string, ActiveStreamState> = {};
        for (const [k, v] of Object.entries(prev)) {
          const age = now - (v.lastUpdated || 0);
          const provInFlight = statusRef.current?.providers?.find((p) => p.name === v.provider)?.in_flight ?? 0;
          const shouldBeDone = v.isDone || (age > 10_000 && provInFlight === 0) || age > 180_000;

          if (age > 180_000) {
            changed = true; // Prune entries older than 3 minutes
            continue;
          }

          if (shouldBeDone !== v.isDone) {
            changed = true;
            next[k] = { ...v, isDone: true };
          } else {
            next[k] = v;
          }
        }
        return changed ? next : prev;
      });
    }, 2000);
    return () => clearInterval(timer);
    // Intentionally empty: status is read via statusRef so the 1s status tick
    // does not tear down and recreate this interval forever (which would stop
    // the 2s pruner tick from ever firing).
  }, []);

  // Ingest recent streams from backend on connect
  useEffect(() => {
    if (!recentStreams || recentStreams.length === 0) return;
    const now = Date.now();
    setActiveStreams((prev) => {
      const next = { ...prev };
      for (const item of recentStreams) {
        const ev = item.event;
        const key = ev.request_id;
        const timestamp = item.timestamp_ms || now;
        const age = now - timestamp;
        if (age <= 180_000) {
          const provInFlight = statusRef.current?.providers?.find((p) => p.name === ev.provider)?.in_flight ?? 0;
          const shouldBeDone = ev.is_done || (age > 10_000 && provInFlight === 0);
          if (!next[key] || (next[key].isDone && timestamp > next[key].lastUpdated) || (!shouldBeDone && next[key].isDone)) {
            next[key] = {
              requestId: ev.request_id,
              virtualModel: ev.virtual_model,
              provider: ev.provider,
              model: ev.model,
              text: ev.delta,
              tokensSoFar: ev.tokens_so_far,
              elapsedMs: ev.elapsed_ms,
              isDone: shouldBeDone,
              error: ev.error,
              speedTokPerSec: ev.elapsed_ms > 150 ? Math.round((ev.tokens_so_far / (ev.elapsed_ms / 1000))) : 0,
              lastUpdated: timestamp,
            };
          }
        }
      }
      return next;
    });
  }, [recentStreams]);

  useEffect(() => {
    if (!liveEvent) return;

    const key = liveEvent.request_id;
    const now = Date.now();
    const speed = liveEvent.elapsed_ms > 150
      ? Math.round((liveEvent.tokens_so_far / (liveEvent.elapsed_ms / 1000)))
      : 0;

    setActiveStreams((prev) => {
      const existing = prev[key];
      const prevText = existing ? existing.text : '';
      const newText = liveEvent.delta ? (prevText + liveEvent.delta).slice(-1000) : prevText;

      const updated = {
        ...prev,
        [key]: {
          requestId: liveEvent.request_id,
          virtualModel: liveEvent.virtual_model || existing?.virtualModel,
          provider: liveEvent.provider || existing?.provider || '',
          model: liveEvent.model || existing?.model || '',
          text: newText,
          tokensSoFar: liveEvent.tokens_so_far,
          elapsedMs: liveEvent.elapsed_ms,
          isDone: liveEvent.is_done,
          error: liveEvent.error,
          speedTokPerSec: speed > 0 ? speed : (existing?.speedTokPerSec || 0),
          lastUpdated: now,
        },
      };

      // Prune entries older than 2 minutes
      const pruned: Record<string, ActiveStreamState> = {};
      for (const [k, v] of Object.entries(updated)) {
        if (now - (v.lastUpdated || 0) <= 120_000) {
          pruned[k] = v;
        }
      }

      return pruned;
    });
  }, [liveEvent]);

  const handleDismissStream = (requestId: string) => {
    setActiveStreams((prev) => {
      if (!prev[requestId]) return prev;
      const copy = { ...prev };
      delete copy[requestId];
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(copy));
      } catch {
        // ignore
      }
      return copy;
    });
  };

  const handleClearCompleted = () => {
    setActiveStreams((prev) => {
      const now = Date.now();
      const next: Record<string, ActiveStreamState> = {};
      for (const [k, v] of Object.entries(prev)) {
        // Keep only truly active streams with recent tokens (<10s)
        if (!v.isDone && (now - v.lastUpdated <= 10_000)) {
          next[k] = v;
        }
      }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        // ignore
      }
      return next;
    });
  };

  const handleClearAllStreams = () => {
    setActiveStreams({});
    try {
      localStorage.removeItem(STORAGE_KEY);
    } catch {
      // ignore
    }
  };

  const activeStreamList = Object.values(activeStreams);

  if (!status) {
    return (
      <Card className="p-12 text-center text-slate-500 font-mono flex flex-col items-center justify-center gap-3">
        <Activity className="w-6 h-6 text-[var(--primary)] animate-spin" />
        <p className="text-sm">Connecting to WireLLM telemetry socket...</p>
      </Card>
    );
  }

  // Snapshot-level in-flight map (provider → count), from the 1s status tick.
  // Drives rung "ACTIVE/busy" badges independently of the WS token feed so
  // non-streaming calls and WS dropouts still show on the ladder.
  const inFlightMap: Record<string, number> = {};
  for (const p of status.providers) {
    if ((p.in_flight || 0) > 0) inFlightMap[p.name] = p.in_flight;
  }

  const formatUptime = (seconds: number) => {
    const d = Math.floor(seconds / (3600 * 24));
    const h = Math.floor((seconds % (3600 * 24)) / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (d > 0) return `${d}d ${h}h ${m}m`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m ${s}s`;
  };

  const formatTokens = (tokens: number) => {
    if (tokens >= 1_000_000_000) return `${(tokens / 1_000_000_000).toFixed(2)}B`;
    if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
    if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}k`;
    return tokens.toLocaleString();
  };

  const handleResetCircuit = async (providerName: string) => {
    try {
      await resetProviderCircuit(providerName);
      onRefresh();
    } catch (err: any) {
      alert(err.message || 'Failed to reset circuit');
    }
  };

  const healthyCount = status.providers.filter((p) => p.circuit.includes('Closed')).length;
  const totalTokens = status.total_input_tokens + status.total_output_tokens;

  const filteredVirtualModels = status.virtual_models.filter((vm) => {
    if (selectedVmTab === 'all') return true;
    return vm.name === selectedVmTab;
  });

  const activePort = window.location.port || '8765';

  return (
    <div className="space-y-6">
      {/* 0. Hero Onboarding: Agent Setup when 0 providers configured, or when toggled */}
      {(status.providers.length === 0 || showAgentSetup) && (
        <AgentSetupHero
          onNavigateToProviders={onNavigateToProviders}
          isCloudOrProtected={isCloudOrProtected}
        />
      )}

      {/* 1. Quick Telemetry Strip using MetricCard */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
        <MetricCard
          label="Gateway Uptime"
          value={formatUptime(status.uptime_secs)}
          icon={<Clock className="w-3.5 h-3.5 text-[var(--primary)]" />}
          accent="primary"
          subtext={
            <span className="text-[var(--primary)]/90 flex items-center gap-1.5">
              <ShieldCheck className="w-3.5 h-3.5 text-[var(--primary)]" />
              <span>Port :{activePort} &bull; SQLite WAL</span>
            </span>
          }
        />

        <MetricCard
          label="Total Tokens"
          value={formatTokens(totalTokens)}
          icon={<Zap className="w-3.5 h-3.5 text-amber-400" />}
          accent="amber"
          subtext={
            <span className="flex items-center gap-1.5">
              <span className="text-amber-300 font-medium">{formatTokens(status.total_input_tokens)} in</span>
              <span className="text-slate-600">&bull;</span>
              <span className="text-[var(--primary)] font-medium">{formatTokens(status.total_output_tokens)} out</span>
            </span>
          }
        />

        <MetricCard
          label="Requests Routed"
          value={status.total_requests.toLocaleString()}
          icon={<Activity className="w-3.5 h-3.5 text-[var(--primary)]" />}
          accent="primary"
          subtext={
            <span className="text-[var(--primary)] flex items-center gap-1.5">
              <span className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--primary)] animate-pulse" />
              <span>&lt;2ms wire overhead</span>
            </span>
          }
        />

        <MetricCard
          label="Circuit Health"
          value={
            <span className="flex items-center gap-2">
              <span className="text-[var(--secondary)] font-bold">{healthyCount}</span>
              <span className="text-xs text-slate-500 font-normal">/ {status.providers.length} online</span>
            </span>
          }
          icon={<CircuitBoard className="w-3.5 h-3.5 text-[var(--secondary)]" />}
          accent="secondary"
          subtext={
            <span className="flex items-center gap-1.5 text-[var(--secondary)]/90">
              <span className="w-1.5 h-1.5 rounded-full bg-[var(--secondary)] shadow-[0_0_6px_var(--secondary)]" />
              <span>Adaptive cascade active</span>
            </span>
          }
        />
      </div>

      {/* ============================================================== */}
      {/* 2. LIVE AGENT DECK: Sci-Fi Neural Reactor & Stream Matrix      */}
      {/* ============================================================== */}
      <LiveAgentDeck
        streams={activeStreamList}
        inFlightMap={inFlightMap}
        onDismiss={handleDismissStream}
        onClearCompleted={handleClearCompleted}
        onClearAll={handleClearAllStreams}
      />

      {/* ============================================================== */}
      {/* 3. THE ROUTING MATRIX: Live Virtual Model Fallback Ladders     */}
      {/* ============================================================== */}
      <Card className="p-5 space-y-5">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-white/[0.06] pb-4">
          <div>
            <div className="flex items-center gap-2.5">
              <div className="p-1.5 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shadow-[0_0_10px_var(--primary-glow-subtle)]">
                <Workflow className="w-4 h-4" />
              </div>
              <h2 className="text-base font-bold text-white tracking-tight flex items-center gap-2 font-mono">
                ROUTING LADDERS <span className="text-slate-500 font-normal text-xs">(Virtual Models)</span>
              </h2>
              <Badge variant="primary">
                <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] animate-ping" />
                Live Wire
              </Badge>
            </div>
            <p className="text-xs text-slate-400 mt-1">
              Deterministic priority ladders. Shows which target is actively receiving traffic, busy shared upstreams, standby failovers, and circuit breakers.
            </p>
          </div>

          {/* Filter Bar & Controls */}
          <div className="flex items-center gap-2 flex-wrap">
            <div className="flex items-center bg-[#030508] p-1 rounded-lg border border-white/[0.08] text-xs">
              <button
                onClick={() => setSelectedVmTab('all')}
                className={`px-3 py-1 rounded-md font-mono text-[11px] font-medium transition-all ${
                  selectedVmTab === 'all'
                    ? 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)] font-bold shadow-[0_0_10px_var(--primary-glow-subtle)]'
                    : 'text-slate-400 hover:text-white border border-transparent'
                }`}
              >
                All Ladders ({status.virtual_models.length})
              </button>
              {status.virtual_models.map((vm) => (
                <button
                  key={vm.name}
                  onClick={() => setSelectedVmTab(vm.name)}
                  className={`px-3 py-1 rounded-md font-mono text-[11px] transition-all ${
                    selectedVmTab === vm.name
                      ? 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)] font-bold shadow-[0_0_10px_var(--primary-glow-subtle)]'
                      : 'text-slate-400 hover:text-white border border-transparent'
                  }`}
                >
                  {vm.name}
                </button>
              ))}
            </div>

            <Button
              variant={filterMode === 'active_blocked' ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setFilterMode(filterMode === 'all' ? 'active_blocked' : 'all')}
              title="Toggle to show only active, busy, primary, or blocked targets"
            >
              <Filter className="w-3 h-3 text-slate-400" />
              {filterMode === 'active_blocked' ? 'Active / Blocked Only' : 'All Targets'}
            </Button>

            <Button
              variant={showPolicyPanel ? 'primary' : 'secondary'}
              size="sm"
              onClick={() => setShowPolicyPanel(!showPolicyPanel)}
              title="Configure upstream header timeouts, overload cooldowns, and circuit breaker thresholds"
              className="font-mono text-xs"
            >
              <Sliders className="w-3 h-3 mr-1 text-[var(--primary)]" />
              Timeouts &amp; Cooldowns
            </Button>

            {status.providers.length > 0 && (
              <Button
                variant={showAgentSetup ? 'primary' : 'secondary'}
                size="sm"
                onClick={() => setShowAgentSetup(!showAgentSetup)}
                title="Toggle Autonomous Agent MCP Setup Guide & Prompt"
                className="font-mono text-xs"
              >
                <Bot className="w-3 h-3 mr-1 text-[var(--primary)]" />
                {showAgentSetup ? 'Hide Agent Guide' : 'Agent MCP Guide'}
              </Button>
            )}

            <Button
              variant="secondary"
              size="icon"
              onClick={onRefresh}
              title="Refresh status"
            >
              <RotateCcw className="w-3.5 h-3.5 text-slate-400 hover:text-white" />
            </Button>
          </div>
        </div>

        {/* Collapsible Timeouts & Cooldowns Live Tuning Panel */}
        {showPolicyPanel && (
          <div className="rounded-xl bg-[#040814] border border-[var(--primary-border)] p-4 space-y-4 shadow-[0_0_25px_var(--primary-glow-subtle)]">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/10 pb-2.5">
              <div>
                <h3 className="text-xs font-bold text-white font-mono flex items-center gap-2">
                  <Sliders className="w-3.5 h-3.5 text-[var(--primary)]" />
                  ROUTING TIMEOUTS &amp; COOLDOWN POLICY (LIVE SQLITE CONFIG)
                </h3>
                <p className="text-[11px] text-slate-400 mt-0.5">
                  Changes apply immediately in-memory and persist across restarts without touching Rust code. Also configurable via MCP (<code className="text-[var(--primary)]">wirellm_set_routing_policy</code>).
                </p>
              </div>
              <div className="flex items-center gap-2">
                {policySavedMsg && (
                  <span className="text-xs font-mono text-emerald-400 font-bold animate-pulse">
                    {policySavedMsg}
                  </span>
                )}
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setRoutingPolicy(DEFAULT_ROUTING_POLICY)}
                  className="text-xs font-mono"
                >
                  Defaults
                </Button>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={handleSaveRoutingPolicy}
                  disabled={savingPolicy}
                  className="text-xs font-mono"
                >
                  <Save className="w-3 h-3 mr-1" />
                  {savingPolicy ? 'Saving...' : 'Save & Apply Live'}
                </Button>
              </div>
            </div>

            <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3 font-mono text-xs">
              {[
                {
                  key: 'stream_header_timeout_secs' as const,
                  label: 'Stream First-Byte Timeout (s)',
                  hint: 'Wait before cascading stream',
                },
                {
                  key: 'nonstream_header_timeout_secs' as const,
                  label: 'Non-Stream Timeout (s)',
                  hint: 'Full non-stream generation budget',
                },
                {
                  key: 'overload_base_cooldown_secs' as const,
                  label: 'Overload Base Cooldown (s)',
                  hint: '1st timeout penalty (e.g. 15s)',
                },
                {
                  key: 'overload_max_cooldown_secs' as const,
                  label: 'Overload Max Cooldown (s)',
                  hint: 'Cap on escalation (e.g. 90s)',
                },
                {
                  key: 'fault_window_secs' as const,
                  label: 'Fault Memory Window (s)',
                  hint: 'Cleared immediately on 200 OK',
                },
                {
                  key: 'circuit_failure_threshold' as const,
                  label: 'Circuit Trip Fails (#)',
                  hint: 'Hard errors before Open (429 excluded)',
                },
                {
                  key: 'circuit_base_cooldown_secs' as const,
                  label: 'Circuit Base Cooldown (s)',
                  hint: 'Initial Open circuit cooldown',
                },
                {
                  key: 'circuit_max_cooldown_secs' as const,
                  label: 'Circuit Max Cooldown (s)',
                  hint: 'Max Open circuit cooldown',
                },
                {
                  key: 'rate_limit_default_cooldown_secs' as const,
                  label: '429 Default Cooldown (s)',
                  hint: 'When Retry-After header missing',
                },
                {
                  key: 'decay_interval_secs' as const,
                  label: 'Background Decay Tick (s)',
                  hint: 'Auto-heal failure counter interval',
                },
              ].map((field) => (
                <label
                  key={field.key}
                  className="bg-[#080e1e] border border-white/10 rounded-lg p-2.5 flex flex-col justify-between gap-1.5 hover:border-[var(--primary-border)] transition-colors"
                >
                  <span className="text-[11px] font-bold text-slate-200 leading-tight">{field.label}</span>
                  <input
                    type="number"
                    min={1}
                    max={3600}
                    value={routingPolicy[field.key]}
                    onChange={(e) =>
                      setRoutingPolicy((prev) => ({
                        ...prev,
                        [field.key]: Math.max(1, parseInt(e.target.value || '1', 10)),
                      }))
                    }
                    className="w-full bg-[#03060f] border border-white/15 rounded px-2 py-1 text-sm font-bold text-[var(--primary)] focus:outline-none focus:border-[var(--primary)]"
                  />
                  <span className="text-[9.5px] text-slate-400 leading-tight">{field.hint}</span>
                </label>
              ))}
            </div>
          </div>
        )}

        {/* Fallback Ladders Grid */}
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
          {filteredVirtualModels.map((vm) => {
            const isFirstHealthyFound = { current: false };
            const now = Date.now();

            const isStreamLive = (s: ActiveStreamState) => {
              if (s.isDone) return false;
              const age = now - (s.lastUpdated || 0);
              if (age <= 10_000) return true;
              if ((inFlightMap[s.provider] ?? 0) > 0 && age <= 90_000) return true;
              return false;
            };

            const evaluatedTargets = vm.targets.map((tgt) => {
              // 1. Direct active stream match STRICTLY for this virtual model
              const isTargetStreaming = activeStreamList.some(
                (s) =>
                  isStreamLive(s) &&
                  s.virtualModel === vm.name &&
                  s.provider === tgt.provider &&
                  s.model === tgt.model
              );

              // 2. Check if this provider is currently occupied by another virtual model
              // or has in-flight requests not belonging to a live stream on this virtual model.
              const otherVmStreams = activeStreamList.filter(
                (s) =>
                  isStreamLive(s) &&
                  s.provider === tgt.provider &&
                  s.virtualModel !== vm.name
              );
              const busyByVirtualModel = Array.from(
                new Set(otherVmStreams.map((s) => s.virtualModel).filter(Boolean))
              ).join(', ');
              const inFlightCount = inFlightMap[tgt.provider] ?? 0;
              const isOccupiedByOtherVm =
                !isTargetStreaming && (inFlightCount > 0 || otherVmStreams.length > 0);

              const evalRes = evaluateTarget(
                tgt.provider,
                tgt.model,
                status.providers,
                isFirstHealthyFound,
                isOccupiedByOtherVm
              );

              const isBlocked = evalRes.status === 'blocked';

              return {
                ...tgt,
                evaluation: {
                  ...evalRes,
                  status: !isBlocked && isTargetStreaming ? ('running' as const) : evalRes.status,
                  busyByVirtualModel: busyByVirtualModel || undefined,
                },
                isStreaming: !isBlocked && isTargetStreaming,
              };
            });

            const activeTarget = evaluatedTargets.find((t) => t.evaluation.status === 'running');
            const primaryStandbyTarget = evaluatedTargets.find((t) => t.evaluation.status === 'primary');

            const runningCount = evaluatedTargets.filter((t) => t.evaluation.status === 'running').length;
            const busyCount = evaluatedTargets.filter((t) => t.evaluation.status === 'busy').length;
            const blockedCount = evaluatedTargets.filter((t) => t.evaluation.status === 'blocked').length;
            const standbyCount = evaluatedTargets.filter((t) => t.evaluation.status === 'standby').length;

            const visibleTargets =
              filterMode === 'active_blocked'
                ? evaluatedTargets.filter((t) => t.evaluation.status !== 'standby')
                : evaluatedTargets;

            return (
              <Card
                key={vm.name}
                hoverEffect
                className="bg-[#050814] border border-white/10 p-4 flex flex-col justify-between space-y-3.5"
              >
                {/* Virtual Model Header Bar - Locked row height to eliminate any layout oscillation */}
                <div className="flex items-center justify-between gap-3 border-b border-white/10 pb-2.5 min-h-[46px]">
                  <div className="flex items-center gap-2 min-w-0 flex-1 overflow-hidden">
                    <span className="text-sm font-bold text-white font-mono tracking-tight flex items-center gap-1.5 whitespace-nowrap shrink-0">
                      <span className="w-2 h-2 rounded-full bg-[var(--primary)] shadow-[0_0_10px_var(--primary)] shrink-0" />
                      {vm.name}
                    </span>
                    {activeTarget ? (
                      <span
                        className="hidden sm:inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] font-mono bg-gradient-to-r from-[rgba(var(--primary-rgb),0.25)] to-[rgba(var(--secondary-rgb),0.12)] border border-[var(--primary)] text-white whitespace-nowrap truncate max-w-[260px] min-w-0 shrink overflow-hidden shadow-[0_0_15px_var(--primary-glow-subtle)] ring-1 ring-[var(--primary)]/30"
                        title={`Active: ${activeTarget.provider}:${activeTarget.model}`}
                      >
                        <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] shadow-[0_0_8px_var(--primary)] shrink-0 animate-ping" />
                        <span className="text-[var(--primary)] font-extrabold tracking-wide shrink-0">ACTIVE: {activeTarget.provider}</span>
                        <span className="text-slate-500 font-bold shrink-0">:</span>
                        <span className="text-white font-semibold truncate">{activeTarget.model}</span>
                      </span>
                    ) : primaryStandbyTarget ? (
                      <span
                        className="hidden sm:inline-flex items-center gap-1.5 px-2 py-1 rounded-md text-[11px] font-mono bg-[#090d18] border border-white/10 text-slate-300 whitespace-nowrap truncate max-w-[260px] min-w-0 shrink overflow-hidden"
                        title={`Standby Next: ${primaryStandbyTarget.provider}:${primaryStandbyTarget.model}`}
                      >
                        <ArrowRight className="w-3 h-3 text-[var(--secondary)] shrink-0" />
                        <span className="text-slate-200 font-semibold shrink-0">NEXT: {primaryStandbyTarget.provider}</span>
                        <span className="text-slate-500 shrink-0">:</span>
                        <span className="text-slate-400 truncate">{primaryStandbyTarget.model}</span>
                      </span>
                    ) : null}
                  </div>

                  {/* Summary Status Badges */}
                  <div className="flex items-center gap-1.5 text-[10px] font-mono shrink-0 whitespace-nowrap">
                    {runningCount > 0 && (
                      <Badge variant="solid-primary" className="animate-pulse shadow-[0_0_12px_var(--primary-glow)] font-black text-black">
                        <Activity className="w-3 h-3 animate-spin text-black shrink-0" />
                        {runningCount} ACTIVE
                      </Badge>
                    )}
                    {busyCount > 0 && (
                      <Badge variant="amber" title="Upstream provider currently busy serving traffic">
                        <Clock className="w-2.5 h-2.5 text-amber-400 animate-pulse shrink-0" />
                        {busyCount} BUSY
                      </Badge>
                    )}
                    <Badge variant="neutral" className="border-white/10 text-slate-300">
                      {standbyCount} STANDBY
                    </Badge>
                    {blockedCount > 0 && (
                      <Badge variant="rose">
                        {blockedCount} BLOCKED
                      </Badge>
                    )}
                  </div>
                </div>
                {vm.description && (
                  <p className="text-[11px] text-slate-400 truncate -mt-1">{vm.description}</p>
                )}

                {/* Ladder List */}
                <div className="space-y-2 max-h-80 overflow-y-auto pr-1">
                  {visibleTargets.map((tgt, idx) => {
                    const ev = tgt.evaluation;
                    const isRunning = ev.status === 'running' || tgt.isStreaming;
                    const isBusy = ev.status === 'busy';
                    const isPrimary = ev.status === 'primary';
                    const isBlocked = ev.status === 'blocked';

                    return (
                      <div
                        key={`${tgt.provider}-${tgt.model}-${idx}`}
                        className={`flex items-center justify-between px-3 py-2.5 rounded-lg text-xs transition-all ${
                          isRunning
                            ? 'bg-gradient-to-r from-[rgba(var(--primary-rgb),0.24)] via-[rgba(var(--primary-rgb),0.12)] to-[rgba(var(--secondary-rgb),0.14)] border-2 border-[var(--primary)] shadow-[0_0_22px_var(--primary-glow-subtle)] ring-1 ring-[var(--primary)]/30'
                            : isBusy
                            ? 'bg-amber-950/15 border border-amber-500/35 hover:border-amber-500/50 shadow-[0_0_12px_rgba(245,158,11,0.08)]'
                            : isPrimary
                            ? 'bg-[#060b17] border border-white/15 hover:border-white/25 shadow-sm'
                            : isBlocked
                            ? 'bg-rose-950/35 border border-rose-500/45 shadow-[0_0_12px_rgba(244,63,94,0.12)]'
                            : 'bg-[#090d18] border border-white/10 hover:border-white/20'
                        }`}
                      >
                        {/* Step Index & Provider */}
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span
                            className={`w-5 h-5 rounded-md text-[10px] font-mono flex items-center justify-center shrink-0 ${
                              isRunning
                                ? 'bg-[var(--primary)] text-black font-black shadow-[0_0_12px_var(--primary)] ring-1 ring-white/50'
                                : isBusy
                                ? 'bg-amber-950/80 text-amber-300 border border-amber-500/50 font-bold'
                                : isPrimary
                                ? 'bg-white/[0.08] text-white border border-white/15 font-bold'
                                : isBlocked
                                ? 'bg-rose-500/25 text-rose-200 border border-rose-400/60 font-bold'
                                : 'bg-white/[0.06] text-slate-400 border border-white/10 font-semibold'
                            }`}
                          >
                            {idx + 1}
                          </span>

                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span
                                className={`font-bold font-mono text-xs truncate ${
                                  isRunning
                                    ? 'text-white drop-shadow-[0_0_8px_var(--primary-glow-subtle)]'
                                    : isBusy
                                    ? 'text-amber-200'
                                    : isPrimary
                                    ? 'text-white'
                                    : isBlocked
                                    ? 'text-rose-100'
                                    : 'text-slate-200'
                                }`}
                              >
                                {tgt.provider}
                              </span>
                              <ChevronRight
                                className={`w-3 h-3 shrink-0 ${
                                  isRunning
                                    ? 'text-[var(--secondary)]'
                                    : isBusy
                                    ? 'text-amber-500/70'
                                    : isBlocked
                                    ? 'text-rose-400/80'
                                    : 'text-slate-600'
                                }`}
                              />
                              <span
                                className={`font-mono text-[11px] truncate max-w-[140px] sm:max-w-[190px] ${
                                  isRunning
                                    ? 'text-[var(--secondary)] font-bold drop-shadow-[0_0_6px_var(--secondary-glow-subtle)]'
                                    : isBusy
                                    ? 'text-amber-300/80 font-medium'
                                    : isPrimary
                                    ? 'text-slate-300 font-medium'
                                    : isBlocked
                                    ? 'text-rose-200/90 font-medium'
                                    : 'text-slate-400'
                                }`}
                                title={tgt.model}
                              >
                                {tgt.model}
                              </span>
                            </div>
                          </div>
                        </div>

                        {/* Status / Failover Tag + High-Contrast Timeout Box */}
                        <div className="flex items-center gap-1.5 shrink-0 font-mono">
                          {isRunning && (
                            <Badge variant="solid-primary" className="shadow-[0_0_12px_var(--primary-glow)] animate-pulse font-black text-black">
                              <Activity className="w-3 h-3 animate-spin text-black shrink-0" />
                              ACTIVE{ev.inFlight > 0 ? ` (${ev.inFlight} req)` : ''}
                            </Badge>
                          )}

                          {isBusy && (
                            <Badge
                              variant="amber"
                              title={
                                ev.busyByVirtualModel
                                  ? `Busy serving virtual model: ${ev.busyByVirtualModel}`
                                  : 'Provider currently busy serving in-flight request(s)'
                              }
                            >
                              <Clock className="w-3 h-3 text-amber-400 animate-pulse shrink-0" />
                              BUSY{ev.busyByVirtualModel ? ` · ${ev.busyByVirtualModel}` : ''}
                              {ev.inFlight > 0 ? ` (${ev.inFlight} req)` : ''}
                            </Badge>
                          )}

                          {isPrimary && !isRunning && !isBusy && (
                            <Badge variant="neutral" className="text-slate-300 border-white/15 bg-white/[0.04]">
                              <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                              {idx === 0 ? 'READY' : `FAILOVER READY (#${idx + 1})`}
                            </Badge>
                          )}

                          {isBlocked && (
                            <div className="flex items-center gap-1.5">
                              <span
                                title={ev.reason}
                                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-rose-500/20 border border-rose-400/50 text-rose-100 text-[10.5px] font-bold tracking-tight cursor-help"
                              >
                                <AlertOctagon className="w-3 h-3 text-rose-300 shrink-0" />
                                <span className="truncate max-w-[135px]">{ev.reason}</span>
                              </span>

                              {ev.cooldownSecs !== undefined && ev.cooldownSecs > 0 && (
                                <span
                                  title={`Timeout / Cooldown remaining: ${ev.cooldownSecs}s`}
                                  className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-md bg-amber-400/25 border-2 border-amber-300 text-amber-100 text-xs font-black tracking-wide shadow-[0_0_14px_rgba(251,191,36,0.45)] shrink-0"
                                >
                                  <Clock className="w-3 h-3 text-amber-300 shrink-0 animate-pulse" />
                                  {ev.cooldownSecs}s
                                </span>
                              )}
                            </div>
                          )}

                          {!isRunning && !isBusy && !isPrimary && !isBlocked && (
                            <span className="text-[10px] text-slate-400 bg-white/[0.04] px-2 py-0.5 rounded border border-white/10 font-mono font-medium">
                              STANDBY #{idx + 1}
                            </span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </Card>
            );
          })}
        </div>
      </Card>

      {/* ============================================================== */}
      {/* 3. Detailed Circuit Breaker & Provider Health Table            */}
      {/* ============================================================== */}
      <Card className="p-5">
        <div className="flex items-center justify-between mb-4 border-b border-white/[0.06] pb-3">
          <div>
            <h2 className="text-base font-bold text-white flex items-center gap-2 font-mono">
              <CircuitBoard className="w-4 h-4 text-[var(--primary)]" />
              PROVIDER CIRCUITS &amp; LIVE METERS
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              Circuit state machine, sliding RPM throttle windows, burst guard timings, and lifetime token consumption.
            </p>
          </div>
        </div>

        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Provider</TableHead>
              <TableHead>Circuit State</TableHead>
              <TableHead>Smartness</TableHead>
              <TableHead>RPM Throttle</TableHead>
              <TableHead>Burst Guard</TableHead>
              <TableHead>In Flight</TableHead>
              <TableHead>Avg Latency</TableHead>
              <TableHead>Tokens (In / Out)</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {status.providers.map((p) => {
              const isHealthy = p.circuit.includes('Closed') && !p.rate_limited;
              const isTripped = p.circuit.includes('Open');
              const isProbing = p.circuit.includes('Half-Open');
              const openSecsMatch = p.circuit.match(/(\d+)s/);
              const cooldownSecs =
                p.rate_limited_until_secs ??
                (openSecsMatch ? parseInt(openSecsMatch[1], 10) : undefined);

              return (
                <TableRow key={String(p.name)}>
                  <TableCell className="font-semibold text-slate-200">
                    <div className="flex items-center gap-2">
                      <span
                        className={`w-2 h-2 rounded-full inline-block ${
                          p.in_flight > 0
                            ? 'bg-[var(--primary)] animate-ping'
                            : isHealthy
                            ? 'bg-[var(--primary)] shadow-[0_0_8px_var(--primary)]'
                            : 'bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.8)]'
                        }`}
                      />
                      <span className="text-xs font-mono font-bold text-white">{p.name}</span>
                    </div>
                    <div className="text-[10px] text-slate-500 font-mono mt-0.5 truncate max-w-xs">
                      {p.models.slice(0, 2).join(', ')}
                      {p.models.length > 2 && ` +${p.models.length - 2} more`}
                    </div>
                  </TableCell>

                  <TableCell>
                    <div className="flex items-center gap-1.5 flex-wrap">
                      {isHealthy && (
                        <Badge variant="primary">
                          <CheckCircle2 className="w-3 h-3 text-[var(--primary)]" />
                          Healthy
                        </Badge>
                      )}
                      {isTripped && (
                        <Badge variant="rose">
                          <XCircle className="w-3 h-3 text-rose-300" />
                          Tripped ({p.failures}f)
                        </Badge>
                      )}
                      {!isTripped && p.rate_limited && (
                        <Badge variant="rose">
                          <AlertOctagon className="w-3 h-3 text-rose-300" />
                          Cooldown ({p.failures}f)
                        </Badge>
                      )}
                      {isProbing && (
                        <Badge variant="amber">
                          <AlertTriangle className="w-3 h-3 text-amber-400" />
                          Probing
                        </Badge>
                      )}
                      {cooldownSecs !== undefined && cooldownSecs > 0 && (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md bg-amber-400/25 border border-amber-300 text-amber-100 font-mono text-xs font-black shadow-[0_0_12px_rgba(251,191,36,0.4)]">
                          <Clock className="w-3 h-3 text-amber-300 animate-pulse" />
                          {cooldownSecs}s
                        </span>
                      )}
                    </div>
                  </TableCell>

                  <TableCell>
                    <div className="flex items-center gap-2">
                      <div className="w-12 bg-white/[0.05] h-1.5 rounded-full overflow-hidden">
                        <div
                          className="bg-[var(--primary)] h-full rounded-full"
                          style={{ width: `${Math.min(100, p.smartness_score)}%` }}
                        />
                      </div>
                      <span className="text-slate-300 font-mono text-[11px]">{p.smartness_score}</span>
                    </div>
                  </TableCell>

                  <TableCell>
                    {p.max_rpm ? (
                      <div className="space-y-1">
                        <div className="flex items-center justify-between text-[11px]">
                          <span className={p.is_rpm_capped ? 'text-amber-400 font-bold' : 'text-slate-300'}>
                            {p.current_rpm} / {p.max_rpm}
                          </span>
                          {p.is_rpm_capped && (
                            <span className="text-[9px] text-amber-400 uppercase font-mono tracking-wide">
                              Capped
                            </span>
                          )}
                        </div>
                        <div className="w-20 bg-white/[0.05] h-1.5 rounded-full overflow-hidden">
                          <div
                            className={`h-full rounded-full transition-all duration-300 ${
                              p.is_rpm_capped ? 'bg-amber-500' : 'bg-[var(--primary)]'
                            }`}
                            style={{ width: `${Math.min(100, (p.current_rpm / p.max_rpm) * 100)}%` }}
                          />
                        </div>
                      </div>
                    ) : (
                      <span className="text-slate-500 text-[11px]">Unlimited</span>
                    )}
                  </TableCell>

                  <TableCell>
                    {p.min_interval_ms ? (
                      p.is_burst_limited ? (
                        <span className="text-amber-400 font-medium text-[11px]">Spacing ({p.min_interval_ms}ms)</span>
                      ) : (
                        <span className="text-[var(--primary)] text-[11px]">Ready ({p.min_interval_ms}ms)</span>
                      )
                    ) : (
                      <span className="text-slate-500 text-[11px]">None</span>
                    )}
                  </TableCell>

                  <TableCell>
                    <span className={p.in_flight > 0 ? 'text-[var(--primary)] font-bold animate-pulse' : 'text-slate-400'}>
                      {p.in_flight}
                    </span>
                  </TableCell>

                  <TableCell>
                    <span className="text-slate-300">
                      {p.avg_latency_ms > 0 ? `${p.avg_latency_ms.toFixed(0)} ms` : '-'}
                    </span>
                  </TableCell>

                  <TableCell className="text-slate-400">
                    <div>
                      {formatTokens(p.lifetime_input_tokens + p.lifetime_output_tokens)}
                    </div>
                    <div className="text-[10px] text-slate-500">
                      {formatTokens(p.current_requests)} reqs
                    </div>
                  </TableCell>

                  <TableCell className="text-right">
                    {(isTripped || p.rate_limited) && (
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => handleResetCircuit(String(p.name))}
                      >
                        Reset
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </Card>
    </div>
  );
};
