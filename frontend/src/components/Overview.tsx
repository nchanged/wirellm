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

interface OverviewProps {
  status: ApiStatusResponse | null;
  onRefresh: () => void;
  liveEvent?: LiveStreamEvent | null;
}

interface TargetEvaluation {
  status: 'running' | 'primary' | 'standby' | 'blocked';
  reason: string;
  inFlight: number;
  circuit: string;
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

  if (inFlight > 0) {
    return {
      status: 'running',
      reason: `${inFlight} req active in-flight`,
      inFlight,
      circuit: p.circuit,
    };
  }

  if (!isFirstHealthyFound.current) {
    isFirstHealthyFound.current = true;
    return {
      status: 'primary',
      reason: 'Primary Healthy Route',
      inFlight: 0,
      circuit: p.circuit,
    };
  }

  return {
    status: 'standby',
    reason: 'Standby Cascade Target',
    inFlight: 0,
    circuit: p.circuit,
  };
}

export const Overview: React.FC<OverviewProps> = ({ status, onRefresh, liveEvent }) => {
  const [selectedVmTab, setSelectedVmTab] = useState<string>('all');
  const [filterMode, setFilterMode] = useState<'all' | 'active_blocked'>('all');
  const [activeStreams, setActiveStreams] = useState<Record<string, ActiveStreamState>>({});

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
      const newText = liveEvent.delta ? (prevText + liveEvent.delta).slice(-800) : prevText;

      return {
        ...prev,
        [key]: {
          requestId: liveEvent.request_id,
          virtualModel: liveEvent.virtual_model,
          provider: liveEvent.provider,
          model: liveEvent.model,
          text: newText,
          tokensSoFar: liveEvent.tokens_so_far,
          elapsedMs: liveEvent.elapsed_ms,
          isDone: liveEvent.is_done,
          error: liveEvent.error,
          speedTokPerSec: speed,
          lastUpdated: now,
        },
      };
    });

    if (liveEvent.is_done) {
      setTimeout(() => {
        setActiveStreams((prev) => {
          if (!prev[key]) return prev;
          const copy = { ...prev };
          delete copy[key];
          return copy;
        });
      }, 7000);
    }
  }, [liveEvent]);

  const handleDismissStream = (requestId: string) => {
    setActiveStreams((prev) => {
      if (!prev[requestId]) return prev;
      const copy = { ...prev };
      delete copy[requestId];
      return copy;
    });
  };

  const handleClearCompleted = () => {
    setActiveStreams((prev) => {
      const next: Record<string, ActiveStreamState> = {};
      for (const [k, v] of Object.entries(prev)) {
        if (!v.isDone) next[k] = v;
      }
      return next;
    });
  };

  const activeStreamList = Object.values(activeStreams);

  if (!status) {
    return (
      <Card className="p-12 text-center text-slate-500 font-mono flex flex-col items-center justify-center gap-3">
        <Activity className="w-6 h-6 text-cyan-400 animate-spin" />
        <p className="text-sm">Connecting to WireLLM telemetry socket...</p>
      </Card>
    );
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
      {/* 1. Quick Telemetry Strip using MetricCard */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
        <MetricCard
          label="Gateway Uptime"
          value={formatUptime(status.uptime_secs)}
          icon={<Clock className="w-3.5 h-3.5 text-cyan-400" />}
          accent="cyan"
          subtext={
            <span className="text-cyan-400/90 flex items-center gap-1.5">
              <ShieldCheck className="w-3.5 h-3.5 text-cyan-400" />
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
              <span className="text-cyan-300 font-medium">{formatTokens(status.total_output_tokens)} out</span>
            </span>
          }
        />

        <MetricCard
          label="Requests Routed"
          value={status.total_requests.toLocaleString()}
          icon={<Activity className="w-3.5 h-3.5 text-cyan-400" />}
          accent="cyan"
          subtext={
            <span className="text-cyan-400 flex items-center gap-1.5">
              <span className="inline-block w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse" />
              <span>&lt;2ms wire overhead</span>
            </span>
          }
        />

        <MetricCard
          label="Circuit Health"
          value={
            <span className="flex items-center gap-2">
              <span className="text-cyan-400">{healthyCount}</span>
              <span className="text-xs text-slate-500 font-normal">/ {status.providers.length} online</span>
            </span>
          }
          icon={<CircuitBoard className="w-3.5 h-3.5 text-indigo-400" />}
          accent="purple"
          subtext={
            <span className="flex items-center gap-1.5 text-indigo-300">
              <span className="w-1.5 h-1.5 rounded-full bg-indigo-400" />
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
      />

      {/* ============================================================== */}
      {/* 3. THE ROUTING MATRIX: Live Virtual Model Fallback Ladders     */}
      {/* ============================================================== */}
      <Card className="p-5 space-y-5">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-white/[0.06] pb-4">
          <div>
            <div className="flex items-center gap-2.5">
              <div className="p-1.5 rounded-lg bg-cyan-500/10 border border-cyan-500/30 text-cyan-400">
                <Workflow className="w-4 h-4" />
              </div>
              <h2 className="text-base font-bold text-white tracking-tight flex items-center gap-2 font-mono">
                ROUTING LADDERS <span className="text-slate-500 font-normal text-xs">(Virtual Models)</span>
              </h2>
              <Badge variant="cyan">
                <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-ping" />
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
                    ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm'
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
                      ? 'bg-cyan-500/20 text-cyan-300 border border-cyan-500/40 shadow-sm'
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

            const evaluatedTargets = vm.targets.map((tgt) => {
              const evalRes = evaluateTarget(
                tgt.provider,
                tgt.model,
                status.providers,
                isFirstHealthyFound
              );

              const isTargetStreaming = activeStreamList.some(
                (s) =>
                  !s.isDone &&
                  (s.virtualModel === vm.name || (!s.virtualModel && s.provider === tgt.provider && s.model === tgt.model)) &&
                  s.provider === tgt.provider &&
                  s.model === tgt.model
              );

              return {
                ...tgt,
                evaluation: {
                  ...evalRes,
                  status: (evalRes.status === 'running' || isTargetStreaming) ? ('running' as const) : evalRes.status,
                },
                isStreaming: isTargetStreaming,
              };
            });

            const activeOrPrimary =
              evaluatedTargets.find((t) => t.evaluation.status === 'running') ||
              evaluatedTargets.find((t) => t.evaluation.status === 'primary');

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
                className="bg-[#040711] p-4 flex flex-col justify-between space-y-3.5"
              >
                {/* Virtual Model Header Bar - Locked row height to eliminate any layout oscillation */}
                <div className="flex items-center justify-between gap-3 border-b border-white/[0.05] pb-2.5 min-h-[46px]">
                  <div className="flex items-center gap-2 min-w-0">
                    <span className="text-sm font-bold text-white font-mono tracking-tight flex items-center gap-1.5 whitespace-nowrap shrink-0">
                      <span className="w-2 h-2 rounded-full bg-[#00f0ff] shadow-[0_0_10px_rgba(0,240,255,0.9)] shrink-0" />
                      {vm.name}
                    </span>
                    {activeOrPrimary && (
                      <span
                        className="hidden sm:inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-[11px] font-mono bg-[#001822] border border-[#00f0ff]/40 text-slate-300 whitespace-nowrap truncate max-w-[210px] shrink-0"
                        title={`${activeOrPrimary.provider}:${activeOrPrimary.model}`}
                      >
                        <ArrowRight className="w-3 h-3 text-[#00f0ff] shrink-0" />
                        <span className="text-[#00f0ff] font-semibold">{activeOrPrimary.provider}</span>
                        <span className="text-slate-600">:</span>
                        <span className="text-slate-300 truncate">{activeOrPrimary.model}</span>
                      </span>
                    )}
                  </div>

                  {/* Summary Status Badges */}
                  <div className="flex items-center gap-1.5 text-[10px] font-mono shrink-0 whitespace-nowrap">
                    {runningCount > 0 && (
                      <Badge variant="cyan" className="animate-pulse">
                        {runningCount} ACTIVE
                      </Badge>
                    )}
                    <Badge variant="neutral">
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
                        className={`flex items-center justify-between px-3 py-2.5 rounded-lg border text-xs transition-all ${
                          isRunning
                            ? 'bg-[#001c24]/40 border-[#00f0ff]/60 shadow-[0_0_15px_rgba(0,240,255,0.25)]'
                            : isPrimary
                            ? 'bg-[#040810] border-cyan-500/40 shadow-sm'
                            : isBlocked
                            ? 'bg-rose-950/15 border-rose-900/40 opacity-75'
                            : 'bg-[#02050c] border-white/[0.04] hover:border-white/[0.08]'
                        }`}
                      >
                        {/* Step Index & Provider */}
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span
                            className={`w-5 h-5 rounded-md text-[10px] font-mono font-bold flex items-center justify-center shrink-0 ${
                              isRunning
                                ? 'bg-[#00f0ff] text-slate-950 font-extrabold shadow-[0_0_10px_rgba(0,240,255,0.9)]'
                                : isPrimary
                                ? 'bg-[#00222a] text-[#00f0ff] border border-[#00f0ff]/50'
                                : isBlocked
                                ? 'bg-rose-950/80 text-rose-400 border border-rose-800/60'
                                : 'bg-slate-800/60 text-slate-400'
                            }`}
                          >
                            {idx + 1}
                          </span>

                          <div className="min-w-0">
                            <div className="flex items-center gap-1.5">
                              <span
                                className={`font-semibold font-mono text-xs truncate ${
                                  isRunning || isPrimary ? 'text-white' : isBlocked ? 'text-slate-400' : 'text-slate-300'
                                }`}
                              >
                                {tgt.provider}
                              </span>
                              <ChevronRight className="w-3 h-3 text-slate-600 shrink-0" />
                              <span
                                className={`font-mono text-[11px] truncate max-w-[150px] sm:max-w-[200px] ${
                                  isRunning || isPrimary ? 'text-[#00f0ff] font-medium' : 'text-slate-400'
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
                            <Badge variant="cyan" className="shadow animate-pulse font-bold">
                              <Activity className="w-3 h-3 animate-spin text-[#00f0ff]" />
                              ACTIVE {ev.inFlight > 0 ? `(${ev.inFlight} req)` : ''}
                            </Badge>
                          )}

                          {isPrimary && !isRunning && (
                            <Badge variant="cyan">
                              <CheckCircle2 className="w-3 h-3 text-[#00f0ff]" />
                              PRIMARY ROUTE
                            </Badge>
                          )}

                          {isBlocked && (
                            <Badge variant="rose" title={ev.reason} className="cursor-help">
                              <AlertOctagon className="w-3 h-3 text-rose-400 shrink-0" />
                              <span className="truncate max-w-[150px]">{ev.reason}</span>
                            </Badge>
                          )}

                          {!isRunning && !isPrimary && !isBlocked && (
                            <span className="text-[10px] text-slate-500 bg-[#010307] px-2 py-0.5 rounded border border-white/[0.04]">
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
              <CircuitBoard className="w-4 h-4 text-cyan-400" />
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
                            ? 'bg-cyan-400 animate-ping'
                            : isHealthy
                            ? 'bg-cyan-400 shadow-[0_0_8px_rgba(6,182,212,0.8)]'
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
                      <Badge variant="cyan">
                        <CheckCircle2 className="w-3 h-3 text-cyan-400" />
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
                          className="bg-cyan-400 h-full rounded-full"
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
                              p.is_rpm_capped ? 'bg-amber-500' : 'bg-cyan-500'
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
                        <span className="text-cyan-400 text-[11px]">Ready ({p.min_interval_ms}ms)</span>
                      )
                    ) : (
                      <span className="text-slate-500 text-[11px]">None</span>
                    )}
                  </TableCell>

                  <TableCell>
                    <span className={p.in_flight > 0 ? 'text-cyan-400 font-bold animate-pulse' : 'text-slate-400'}>
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
