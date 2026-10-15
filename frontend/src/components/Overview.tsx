import React, { useEffect, useState } from 'react';
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
  status: 'running' | 'primary' | 'standby' | 'blocked';
  reason: string;
  inFlight: number;
  circuit: string;
  /** Busy only via the status-tick in-flight counter (no token stream seen) */
  providerBusyOnly?: boolean;
}

function evaluateTarget(
  providerName: string,
  _modelName: string,
  providers: ProviderRuntimeDetail[],
  isFirstHealthyFound: { current: boolean }
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

  const isTripped = p.circuit.includes('Open');
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
    return {
      status: 'blocked',
      reason: `Circuit Open (${p.failures} fails)`,
      inFlight,
      circuit: p.circuit,
    };
  }

  if (p.rate_limited) {
    return {
      status: 'blocked',
      reason: `429 Rate-Limited ${p.rate_limited_until_secs ? `(${p.rate_limited_until_secs}s)` : ''}`,
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
      reason: `Burst Spacing (${p.min_interval_ms}ms cooldown)`,
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
              isDone: v.isDone || age > 10_000, // Mark done if inactive for >10s
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

  // Periodic pruner: mark streams inactive for >10s as done, and remove streams older than 2 minutes
  useEffect(() => {
    const timer = setInterval(() => {
      setActiveStreams((prev) => {
        const now = Date.now();
        let changed = false;
        const next: Record<string, ActiveStreamState> = {};
        for (const [k, v] of Object.entries(prev)) {
          const age = now - (v.lastUpdated || 0);
          const shouldBeDone = v.isDone || age > 10_000;

          if (age > 120_000) {
            changed = true; // Prune entries older than 2 minutes
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
          const shouldBeDone = ev.is_done || age > 10_000;
          if (!next[key] || (next[key].isDone && timestamp > next[key].lastUpdated)) {
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
      {/* 2. LIVE AGENT DECK: Real-time Intercepted Agent Stream View    */}
      {/* ============================================================== */}
      <LiveAgentDeck
        streams={activeStreamList}
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
              Deterministic priority ladders. Shows which target is actively receiving traffic, standby failovers, and circuit breakers.
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
              title="Toggle to show only active, primary, or blocked targets"
            >
              <Filter className="w-3 h-3 text-slate-400" />
              {filterMode === 'active_blocked' ? 'Active / Blocked Only' : 'All Targets'}
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

        {/* Fallback Ladders Grid */}
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
          {filteredVirtualModels.map((vm) => {
            const isFirstHealthyFound = { current: false };
            const now = Date.now();

            const evaluatedTargets = vm.targets.map((tgt) => {
              const evalRes = evaluateTarget(
                tgt.provider,
                tgt.model,
                status.providers,
                isFirstHealthyFound
              );

              const isBlocked = evalRes.status === 'blocked';

              // 1. Direct active stream match STRICTLY for this virtual model (received tokens within last 10s)
              const isTargetStreaming =
                !isBlocked &&
                activeStreamList.some(
                  (s) =>
                    !s.isDone &&
                    (now - (s.lastUpdated || 0) <= 10_000) &&
                    s.virtualModel === vm.name &&
                    s.provider === tgt.provider &&
                    s.model === tgt.model
                );

              // 2. Restart-proof + non-stream-aware: a rung is also BUSY when the
              // status snapshot reports in-flight work on its provider — covers
              // WS dropout and non-streaming calls (0 token events). Note: when
              // the WS is down the snapshot refreshes per ~4s poll, so a busy
              // badge can linger up to ~4s after the burst ends in that state.
              const inFlightBusy = !isBlocked && (inFlightMap[tgt.provider] ?? 0) > 0;
              const isBusy = isTargetStreaming || inFlightBusy;

              return {
                ...tgt,
                evaluation: {
                  ...evalRes,
                  status: isBusy ? ('running' as const) : evalRes.status,
                  providerBusyOnly: inFlightBusy && !isTargetStreaming,
                },
                isStreaming: isBusy,
              };
            });

            const activeTarget = evaluatedTargets.find((t) => t.evaluation.status === 'running');
            const primaryStandbyTarget = evaluatedTargets.find((t) => t.evaluation.status === 'primary');

            const runningCount = evaluatedTargets.filter((t) => t.evaluation.status === 'running').length;
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
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-sm font-bold text-white font-mono tracking-tight flex items-center gap-1.5 whitespace-nowrap shrink-0">
                      <span className="w-2 h-2 rounded-full bg-[var(--primary)] shadow-[0_0_10px_var(--primary)] shrink-0" />
                      {vm.name}
                    </span>
                    {activeTarget ? (
                      <span
                        className="hidden sm:inline-flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] font-mono bg-gradient-to-r from-[rgba(var(--primary-rgb),0.25)] to-[rgba(var(--secondary-rgb),0.12)] border border-[var(--primary)] text-white whitespace-nowrap truncate max-w-[260px] shrink-0 shadow-[0_0_15px_var(--primary-glow-subtle)] ring-1 ring-[var(--primary)]/30"
                        title={`Active: ${activeTarget.provider}:${activeTarget.model}`}
                      >
                        <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] shadow-[0_0_8px_var(--primary)] shrink-0 animate-ping" />
                        <span className="text-[var(--primary)] font-extrabold tracking-wide">ACTIVE: {activeTarget.provider}</span>
                        <span className="text-slate-500 font-bold">:</span>
                        <span className="text-white font-semibold truncate">{activeTarget.model}</span>
                      </span>
                    ) : primaryStandbyTarget ? (
                      <span
                        className="hidden sm:inline-flex items-center gap-1.5 px-2 py-1 rounded-md text-[11px] font-mono bg-[#090d18] border border-white/10 text-slate-300 whitespace-nowrap truncate max-w-[260px] shrink-0"
                        title={`Standby Next: ${primaryStandbyTarget.provider}:${primaryStandbyTarget.model}`}
                      >
                        <ArrowRight className="w-3 h-3 text-[var(--secondary)] shrink-0" />
                        <span className="text-slate-200 font-semibold">NEXT: {primaryStandbyTarget.provider}</span>
                        <span className="text-slate-500">:</span>
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
                    const isPrimary = ev.status === 'primary';
                    const isBlocked = ev.status === 'blocked';

                    return (
                      <div
                        key={`${tgt.provider}-${tgt.model}-${idx}`}
                        className={`flex items-center justify-between px-3 py-2.5 rounded-lg text-xs transition-all ${
                          isRunning
                            ? 'bg-gradient-to-r from-[rgba(var(--primary-rgb),0.24)] via-[rgba(var(--primary-rgb),0.12)] to-[rgba(var(--secondary-rgb),0.14)] border-2 border-[var(--primary)] shadow-[0_0_22px_var(--primary-glow-subtle)] ring-1 ring-[var(--primary)]/30'
                            : isPrimary
                            ? 'bg-[#060b17] border border-white/15 hover:border-white/25 shadow-sm'
                            : isBlocked
                            ? 'bg-rose-950/20 border border-rose-900/40 opacity-80'
                            : 'bg-[#090d18] border border-white/10 hover:border-white/20'
                        }`}
                      >
                        {/* Step Index & Provider */}
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span
                            className={`w-5 h-5 rounded-md text-[10px] font-mono flex items-center justify-center shrink-0 ${
                              isRunning
                                ? 'bg-[var(--primary)] text-black font-black shadow-[0_0_12px_var(--primary)] ring-1 ring-white/50'
                                : isPrimary
                                ? 'bg-white/[0.08] text-white border border-white/15 font-bold'
                                : isBlocked
                                ? 'bg-rose-950/80 text-rose-400 border border-rose-800/60 font-semibold'
                                : 'bg-white/[0.06] text-slate-400 border border-white/10 font-semibold'
                            }`}
                          >
                            {idx + 1}
                          </span>

                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span
                                className={`font-bold font-mono text-xs truncate ${
                                  isRunning ? 'text-white drop-shadow-[0_0_8px_var(--primary-glow-subtle)]' : isPrimary ? 'text-white' : isBlocked ? 'text-slate-400' : 'text-slate-200'
                                }`}
                              >
                                {tgt.provider}
                              </span>
                              <ChevronRight className={`w-3 h-3 shrink-0 ${isRunning ? 'text-[var(--secondary)]' : 'text-slate-600'}`} />
                              <span
                                className={`font-mono text-[11px] truncate max-w-[150px] sm:max-w-[200px] ${
                                  isRunning ? 'text-[var(--secondary)] font-bold drop-shadow-[0_0_6px_var(--secondary-glow-subtle)]' : isPrimary ? 'text-slate-300 font-medium' : 'text-slate-400'
                                }`}
                                title={tgt.model}
                              >
                                {tgt.model}
                              </span>
                            </div>
                          </div>
                        </div>

                        {/* Status / Failover Tag */}
                        <div className="flex items-center gap-2 shrink-0 font-mono">
                          {isRunning && (
                            <Badge variant="solid-primary" className="shadow-[0_0_12px_var(--primary-glow)] animate-pulse font-black text-black">
                              <Activity className="w-3 h-3 animate-spin text-black shrink-0" />
                              {ev.providerBusyOnly ? 'IN-FLIGHT' : 'ACTIVE'}
                              {ev.inFlight > 0 ? ` (${ev.inFlight} req)` : ''}
                            </Badge>
                          )}

                          {isPrimary && !isRunning && (
                            <Badge variant="neutral" className="text-slate-300 border-white/15 bg-white/[0.04]">
                              <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                              {idx === 0 ? 'READY' : `FAILOVER READY (#${idx + 1})`}
                            </Badge>
                          )}

                          {isBlocked && (
                            <Badge variant="rose" title={ev.reason} className="cursor-help">
                              <AlertOctagon className="w-3 h-3 text-rose-400 shrink-0" />
                              <span className="truncate max-w-[150px]">{ev.reason}</span>
                            </Badge>
                          )}

                          {!isRunning && !isPrimary && !isBlocked && (
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
              const isHealthy = p.circuit.includes('Closed');
              const isTripped = p.circuit.includes('Open');
              const isProbing = p.circuit.includes('Half-Open');

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
                    {isHealthy && (
                      <Badge variant="primary">
                        <CheckCircle2 className="w-3 h-3 text-[var(--primary)]" />
                        Healthy
                      </Badge>
                    )}
                    {isTripped && (
                      <Badge variant="rose" className="animate-pulse">
                        <XCircle className="w-3 h-3 text-rose-400" />
                        Tripped ({p.failures}f)
                      </Badge>
                    )}
                    {isProbing && (
                      <Badge variant="amber">
                        <AlertTriangle className="w-3 h-3 text-amber-400" />
                        Probing
                      </Badge>
                    )}
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
                    {isTripped && (
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
