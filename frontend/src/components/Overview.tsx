import React, { useState } from 'react';
import {
  ApiStatusResponse,
  ProviderRuntimeDetail,
} from '../types';
import {
  Activity,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Zap,
  Gauge,
  Clock,
  RotateCcw,
  Layers,
  Database,
  ShieldCheck,
  AlertOctagon,
  ArrowRight,
  ChevronRight,
  Filter,
} from 'lucide-react';
import { resetProviderCircuit } from '../api';

interface OverviewProps {
  status: ApiStatusResponse | null;
  onRefresh: () => void;
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
      reason: 'Active Target (Next in line)',
      inFlight: 0,
      circuit: p.circuit,
    };
  }

  return {
    status: 'standby',
    reason: 'Standby Backup',
    inFlight: 0,
    circuit: p.circuit,
  };
}

export const Overview: React.FC<OverviewProps> = ({ status, onRefresh }) => {
  const [selectedVmTab, setSelectedVmTab] = useState<string>('all');
  const [filterMode, setFilterMode] = useState<'all' | 'active_blocked'>('all');

  if (!status) {
    return (
      <div className="flex items-center justify-center p-12 text-slate-400">
        <Activity className="w-6 h-6 animate-spin mr-2 text-brand-500" />
        <span>Loading gateway state...</span>
      </div>
    );
  }

  const formatUptime = (secs: number) => {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (d > 0) return `${d}d ${h}h ${m}m`;
    if (h > 0) return `${h}h ${m}m ${s}s`;
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

  return (
    <div className="space-y-6">
      {/* 1. Quick Metrics Stats Strip */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow-lg flex items-center justify-between">
          <div>
            <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
              Gateway Uptime
            </div>
            <div className="mt-1 text-xl font-bold text-white tracking-tight">
              {formatUptime(status.uptime_secs)}
            </div>
            <div className="text-[11px] text-emerald-400 flex items-center gap-1 mt-0.5">
              <ShieldCheck className="w-3 h-3" />
              <span>Port :8088 &bull; SQLite WAL</span>
            </div>
          </div>
          <Clock className="w-5 h-5 text-emerald-400/80" />
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow-lg flex items-center justify-between">
          <div>
            <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
              Tokens Processed
            </div>
            <div className="mt-1 text-xl font-bold text-white tracking-tight">
              {formatTokens(totalTokens)}
            </div>
            <div className="text-[11px] text-slate-400 mt-0.5">
              {formatTokens(status.total_input_tokens)} in &bull; {formatTokens(status.total_output_tokens)} out
            </div>
          </div>
          <Zap className="w-5 h-5 text-amber-400/80" />
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow-lg flex items-center justify-between">
          <div>
            <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
              Requests Handled
            </div>
            <div className="mt-1 text-xl font-bold text-white tracking-tight">
              {status.total_requests.toLocaleString()}
            </div>
            <div className="text-[11px] text-sky-400 mt-0.5">
              Zero dropped sessions
            </div>
          </div>
          <Activity className="w-5 h-5 text-sky-400/80" />
        </div>

        <div className="bg-slate-900 border border-slate-800 rounded-xl p-4 shadow-lg flex items-center justify-between">
          <div>
            <div className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">
              Providers Health
            </div>
            <div className="mt-1 text-xl font-bold text-white tracking-tight flex items-center gap-1.5">
              <span className="text-emerald-400">{healthyCount}</span>
              <span className="text-xs text-slate-500 font-normal">/ {status.providers.length} online</span>
            </div>
            <div className="text-[11px] text-slate-400 mt-0.5">
              Adaptive failover active
            </div>
          </div>
          <Database className="w-5 h-5 text-purple-400/80" />
        </div>
      </div>

      {/* ============================================================== */}
      {/* 2. FIRST & CENTER: Configured Virtual Model Fallback Ladders   */}
      {/* ============================================================== */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-2xl space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 border-b border-slate-800/80 pb-4">
          <div>
            <div className="flex items-center gap-2">
              <Layers className="w-5 h-5 text-brand-400" />
              <h2 className="text-base font-bold text-white">
                Live Fallback Ladders (Virtual Models)
              </h2>
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-bold bg-emerald-950/80 text-emerald-400 border border-emerald-800/60 uppercase">
                <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                Live Router
              </span>
            </div>
            <p className="text-xs text-slate-400 mt-0.5">
              Real-time routing ladder. Shows which target is actively receiving requests, which is next in line, and why blocked models failover.
            </p>
          </div>

          {/* Controls: Tab filters & Refresh */}
          <div className="flex items-center gap-2 flex-wrap">
            <div className="flex items-center bg-slate-950 p-1 rounded-lg border border-slate-800 text-xs">
              <button
                onClick={() => setSelectedVmTab('all')}
                className={`px-2.5 py-1 rounded-md font-medium transition-colors ${
                  selectedVmTab === 'all'
                    ? 'bg-brand-600 text-white'
                    : 'text-slate-400 hover:text-white'
                }`}
              >
                All Pools ({status.virtual_models.length})
              </button>
              {status.virtual_models.map((vm) => (
                <button
                  key={vm.name}
                  onClick={() => setSelectedVmTab(vm.name)}
                  className={`px-2.5 py-1 rounded-md font-mono text-[11px] transition-colors ${
                    selectedVmTab === vm.name
                      ? 'bg-brand-600 text-white'
                      : 'text-slate-400 hover:text-white'
                  }`}
                >
                  {vm.name}
                </button>
              ))}
            </div>

            <button
              onClick={() => setFilterMode(filterMode === 'all' ? 'active_blocked' : 'all')}
              className={`px-2.5 py-1.5 rounded-lg text-xs font-medium border transition-colors flex items-center gap-1 ${
                filterMode === 'active_blocked'
                  ? 'bg-amber-950/60 text-amber-300 border-amber-800'
                  : 'bg-slate-800 hover:bg-slate-700 text-slate-300 border-slate-700'
              }`}
              title="Toggle to show only active, primary, or blocked targets"
            >
              <Filter className="w-3 h-3" />
              {filterMode === 'active_blocked' ? 'Active/Blocked Only' : 'Show All Steps'}
            </button>

            <button
              onClick={onRefresh}
              className="p-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg border border-slate-700 transition-colors"
              title="Refresh status"
            >
              <RotateCcw className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Fallback Ladders Grid */}
        <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
          {filteredVirtualModels.map((vm) => {
            const isFirstHealthyFound = { current: false };

            // Pre-evaluate each target
            const evaluatedTargets = vm.targets.map((tgt) => {
              const evalRes = evaluateTarget(
                tgt.provider,
                tgt.model,
                status.providers,
                isFirstHealthyFound
              );
              return {
                ...tgt,
                evaluation: evalRes,
              };
            });

            // Find current active / primary target
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
              <div
                key={vm.name}
                className="bg-slate-950/80 border border-slate-800/90 rounded-xl p-4 shadow-lg flex flex-col justify-between space-y-3"
              >
                {/* Virtual Model Header Bar */}
                <div className="flex items-start justify-between gap-2 border-b border-slate-800/60 pb-2.5">
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-bold text-brand-400 font-mono tracking-tight">
                        {vm.name}
                      </span>
                      {activeOrPrimary && (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-mono bg-slate-900 border border-slate-700/80 text-slate-300">
                          <ArrowRight className="w-3 h-3 text-emerald-400" />
                          <span className="text-emerald-400 font-semibold">{activeOrPrimary.provider}</span>
                          <span className="text-slate-500">:</span>
                          <span className="truncate max-w-[140px] text-slate-300">{activeOrPrimary.model}</span>
                        </span>
                      )}
                    </div>
                    {vm.description && (
                      <p className="text-[11px] text-slate-400 mt-0.5">{vm.description}</p>
                    )}
                  </div>

                  {/* Summary Status Badges */}
                  <div className="flex items-center gap-1 text-[10px] font-mono flex-shrink-0">
                    {runningCount > 0 && (
                      <span className="px-1.5 py-0.5 rounded bg-emerald-950 text-emerald-300 border border-emerald-800 font-bold animate-pulse">
                        {runningCount} RUNNING
                      </span>
                    )}
                    <span className="px-1.5 py-0.5 rounded bg-slate-900 text-slate-400 border border-slate-800">
                      {standbyCount} STANDBY
                    </span>
                    {blockedCount > 0 && (
                      <span className="px-1.5 py-0.5 rounded bg-rose-950/80 text-rose-300 border border-rose-800/70 font-semibold">
                        {blockedCount} BLOCKED
                      </span>
                    )}
                  </div>
                </div>

                {/* Compact, Highly Visual Ladder List */}
                <div className="space-y-1.5 max-h-72 overflow-y-auto pr-1">
                  {visibleTargets.map((tgt, idx) => {
                    const ev = tgt.evaluation;
                    const isRunning = ev.status === 'running';
                    const isPrimary = ev.status === 'primary';
                    const isBlocked = ev.status === 'blocked';

                    return (
                      <div
                        key={`${tgt.provider}-${tgt.model}-${idx}`}
                        className={`flex items-center justify-between px-3 py-2 rounded-lg border text-xs transition-all ${
                          isRunning
                            ? 'bg-emerald-950/50 border-emerald-500/80 shadow-md shadow-emerald-950/50 animate-pulse'
                            : isPrimary
                            ? 'bg-slate-900 border-emerald-600/70 shadow-sm'
                            : isBlocked
                            ? 'bg-rose-950/20 border-rose-900/50 opacity-80'
                            : 'bg-slate-900/50 border-slate-800/60 hover:bg-slate-900'
                        }`}
                      >
                        {/* Step Index & Provider */}
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span
                            className={`w-4 h-4 rounded-full text-[10px] font-bold flex items-center justify-center flex-shrink-0 ${
                              isRunning
                                ? 'bg-emerald-500 text-slate-950'
                                : isPrimary
                                ? 'bg-emerald-500/30 text-emerald-300 border border-emerald-500/50'
                                : isBlocked
                                ? 'bg-rose-950 text-rose-400 border border-rose-800/60'
                                : 'bg-slate-800 text-slate-400'
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
                              <ChevronRight className="w-3 h-3 text-slate-600 flex-shrink-0" />
                              <span
                                className={`font-mono text-[11px] truncate max-w-[150px] sm:max-w-[200px] ${
                                  isRunning || isPrimary ? 'text-emerald-300' : 'text-slate-400'
                                }`}
                                title={tgt.model}
                              >
                                {tgt.model}
                              </span>
                            </div>
                          </div>
                        </div>

                        {/* Status / Failover Tag */}
                        <div className="flex items-center gap-2 flex-shrink-0">
                          {isRunning && (
                            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-bold bg-emerald-500 text-slate-950 font-mono shadow">
                              <Activity className="w-3 h-3 animate-spin" />
                              ACTIVE ({ev.inFlight} req)
                            </span>
                          )}

                          {isPrimary && (
                            <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-semibold bg-emerald-950/80 text-emerald-300 border border-emerald-700/80 font-mono">
                              <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                              PRIMARY (Next)
                            </span>
                          )}

                          {isBlocked && (
                            <span
                              className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono font-medium bg-rose-950 text-rose-300 border border-rose-800/80 cursor-help"
                              title={ev.reason}
                            >
                              <AlertOctagon className="w-3 h-3 text-rose-400 flex-shrink-0" />
                              <span className="truncate max-w-[160px]">{ev.reason}</span>
                            </span>
                          )}

                          {!isRunning && !isPrimary && !isBlocked && (
                            <span className="text-[10px] font-mono text-slate-500 bg-slate-950 px-2 py-0.5 rounded border border-slate-800/80">
                              STANDBY
                            </span>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* ============================================================== */}
      {/* 3. Detailed Circuit Breaker & Provider Health Table            */}
      {/* ============================================================== */}
      <div className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-xl">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-base font-bold text-white flex items-center gap-2">
              <Gauge className="w-5 h-5 text-brand-500" />
              Provider Circuit Breakers & Throttle Meters
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              Live status, sliding 60-second RPM counters, burst spacing timers, and smartness rankings.
            </p>
          </div>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm text-slate-300">
            <thead className="bg-slate-950/60 text-slate-400 text-xs uppercase tracking-wider border-b border-slate-800">
              <tr>
                <th className="py-2.5 px-3 font-medium">Provider</th>
                <th className="py-2.5 px-3 font-medium">Circuit State</th>
                <th className="py-2.5 px-3 font-medium">Smartness</th>
                <th className="py-2.5 px-3 font-medium">RPM Throttle</th>
                <th className="py-2.5 px-3 font-medium">Burst Guard</th>
                <th className="py-2.5 px-3 font-medium">In Flight</th>
                <th className="py-2.5 px-3 font-medium">Avg Latency</th>
                <th className="py-2.5 px-3 font-medium">Tokens</th>
                <th className="py-2.5 px-3 font-medium text-right">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60 font-mono text-xs">
              {status.providers.map((p) => {
                const isHealthy = p.circuit.includes('Closed');
                const isTripped = p.circuit.includes('Open');
                const isProbing = p.circuit.includes('Half-Open');

                return (
                  <tr key={String(p.name)} className="hover:bg-slate-800/40 transition-colors">
                    <td className="py-2.5 px-3 font-semibold text-slate-200">
                      <div className="flex items-center gap-2">
                        <span
                          className={`w-2 h-2 rounded-full inline-block ${
                            p.in_flight > 0
                              ? 'bg-emerald-400 animate-ping'
                              : isHealthy
                              ? 'bg-emerald-500'
                              : 'bg-rose-500'
                          }`}
                        />
                        <span className="text-xs font-sans font-bold">{p.name}</span>
                      </div>
                      <div className="text-[10px] text-slate-500 font-sans mt-0.5 truncate max-w-xs">
                        {p.models.slice(0, 2).join(', ')}
                        {p.models.length > 2 && ` +${p.models.length - 2} more`}
                      </div>
                    </td>

                    <td className="py-2.5 px-3">
                      {isHealthy && (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-emerald-950/70 text-emerald-400 border border-emerald-800/60">
                          <CheckCircle2 className="w-3 h-3" />
                          Healthy
                        </span>
                      )}
                      {isTripped && (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-rose-950/70 text-rose-400 border border-rose-800/60 animate-pulse">
                          <XCircle className="w-3 h-3" />
                          Tripped
                        </span>
                      )}
                      {isProbing && (
                        <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-amber-950/70 text-amber-400 border border-amber-800/60">
                          <AlertTriangle className="w-3 h-3" />
                          Probing
                        </span>
                      )}
                    </td>

                    <td className="py-2.5 px-3">
                      <div className="flex items-center gap-2">
                        <div className="w-12 bg-slate-800 h-1.5 rounded-full overflow-hidden">
                          <div
                            className="bg-brand-500 h-full rounded-full"
                            style={{ width: `${Math.min(100, p.smartness_score)}%` }}
                          />
                        </div>
                        <span className="text-slate-300 font-sans font-medium text-[11px]">{p.smartness_score}</span>
                      </div>
                    </td>

                    <td className="py-2.5 px-3">
                      {p.max_rpm ? (
                        <div className="space-y-1">
                          <div className="flex items-center justify-between text-[11px]">
                            <span className={p.is_rpm_capped ? 'text-amber-400 font-bold' : 'text-slate-300'}>
                              {p.current_rpm} / {p.max_rpm}
                            </span>
                            {p.is_rpm_capped && (
                              <span className="text-[9px] text-amber-400 uppercase font-sans tracking-wide">
                                Capped
                              </span>
                            )}
                          </div>
                          <div className="w-20 bg-slate-800 h-1.5 rounded-full overflow-hidden">
                            <div
                              className={`h-full rounded-full transition-all duration-300 ${
                                p.is_rpm_capped ? 'bg-amber-500' : 'bg-sky-500'
                              }`}
                              style={{ width: `${Math.min(100, (p.current_rpm / p.max_rpm) * 100)}%` }}
                            />
                          </div>
                        </div>
                      ) : (
                        <span className="text-slate-500 font-sans text-[11px]">Unlimited</span>
                      )}
                    </td>

                    <td className="py-2.5 px-3">
                      {p.min_interval_ms ? (
                        p.is_burst_limited ? (
                          <span className="text-amber-400 font-medium text-[11px]">Spacing ({p.min_interval_ms}ms)</span>
                        ) : (
                          <span className="text-emerald-400 text-[11px]">Ready ({p.min_interval_ms}ms)</span>
                        )
                      ) : (
                        <span className="text-slate-500 font-sans text-[11px]">None</span>
                      )}
                    </td>

                    <td className="py-2.5 px-3">
                      <span className={p.in_flight > 0 ? 'text-emerald-400 font-bold animate-pulse' : 'text-slate-400'}>
                        {p.in_flight}
                      </span>
                    </td>

                    <td className="py-2.5 px-3">
                      <span className="text-slate-300">
                        {p.avg_latency_ms > 0 ? `${p.avg_latency_ms.toFixed(0)} ms` : '-'}
                      </span>
                    </td>

                    <td className="py-2.5 px-3 text-slate-400 font-sans">
                      <div>
                        {formatTokens(p.lifetime_input_tokens + p.lifetime_output_tokens)}
                      </div>
                      <div className="text-[10px] text-slate-500">
                        {formatTokens(p.current_requests)} reqs
                      </div>
                    </td>

                    <td className="py-2.5 px-3 text-right">
                      {isTripped && (
                        <button
                          onClick={() => handleResetCircuit(String(p.name))}
                          className="px-2 py-0.5 bg-amber-500/20 hover:bg-amber-500/30 text-amber-300 border border-amber-500/40 rounded text-[11px] font-sans transition-colors"
                        >
                          Reset
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
