import React, { useEffect, useState } from 'react';
import { Terminal, CheckCircle2, AlertOctagon, X, Zap, Wrench } from 'lucide-react';
import { Badge, Button, cn } from '../ui';

export interface ActiveStreamState {
  requestId: string;
  virtualModel?: string;
  provider: string;
  model: string;
  text: string;
  tokensSoFar: number;
  elapsedMs: number;
  isDone: boolean;
  error?: string;
  speedTokPerSec: number;
  lastUpdated: number;
}

interface LiveAgentDeckProps {
  streams: ActiveStreamState[];
  onDismiss: (requestId: string) => void;
  onClearCompleted: () => void;
}

function formatTimeAgo(timestampMs: number, now: number): string {
  const diffSec = Math.max(0, Math.floor((now - timestampMs) / 1000));
  if (diffSec < 3) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  return `${diffHr}h ago`;
}

// Color decay calculator: bright cyan -> sky blue -> slate gray -> dark gray
function getDecayStyle(isDone: boolean, ageMs: number) {
  if (!isDone) {
    return {
      glow: '#00f0ff',
      border: 'rgba(0, 240, 255, 0.45)',
      badge: 'bg-[#00222b] text-[#00f0ff] border-[#00f0ff]/50 shadow-[0_0_8px_rgba(0,240,255,0.3)]',
      providerText: 'text-white',
      modelText: 'text-cyan-200',
      speedText: 'text-[#00f0ff]',
      promptColor: 'text-[#00f0ff]',
      tickerText: 'text-cyan-50',
      activeGlow: 'shadow-[0_0_15px_rgba(0,240,255,0.12)]',
    };
  }

  const sec = ageMs / 1000;
  if (sec < 20) {
    // Fresh complete (< 20s)
    return {
      glow: '#00d5e8',
      border: 'rgba(0, 213, 232, 0.25)',
      badge: 'bg-[#011822] text-cyan-300 border-cyan-500/30',
      providerText: 'text-slate-100',
      modelText: 'text-slate-300',
      speedText: 'text-cyan-300',
      promptColor: 'text-cyan-400',
      tickerText: 'text-slate-200',
      activeGlow: '',
    };
  }
  if (sec < 60) {
    // 20s - 60s: Sky blue transition
    return {
      glow: '#38bdf8',
      border: 'rgba(56, 189, 248, 0.2)',
      badge: 'bg-sky-950/40 text-sky-400 border-sky-600/30',
      providerText: 'text-slate-200',
      modelText: 'text-slate-400',
      speedText: 'text-sky-400',
      promptColor: 'text-sky-400',
      tickerText: 'text-slate-300',
      activeGlow: '',
    };
  }
  if (sec < 150) {
    // 1m - 2.5m: Slate gray
    return {
      glow: '#64748b',
      border: 'rgba(100, 116, 139, 0.15)',
      badge: 'bg-slate-900/60 text-slate-400 border-slate-700/30',
      providerText: 'text-slate-300',
      modelText: 'text-slate-500',
      speedText: 'text-slate-400',
      promptColor: 'text-slate-500',
      tickerText: 'text-slate-400',
      activeGlow: '',
    };
  }
  // > 2.5m: Dark muted gray
  return {
    glow: '#334155',
    border: 'rgba(51, 65, 85, 0.12)',
    badge: 'bg-[#060a12] text-slate-500 border-slate-800',
    providerText: 'text-slate-400',
    modelText: 'text-slate-600',
    speedText: 'text-slate-500',
    promptColor: 'text-slate-600',
    tickerText: 'text-slate-500',
    activeGlow: '',
  };
}

// Extract last non-empty line / tool call, capped at ~180 chars
function getTailSnippet(text: string): { snippet: string; isTool: boolean; toolName?: string } {
  if (!text) return { snippet: 'Listening for upstream tokens...', isTool: false };
  const lines = text.trimEnd().split('\n').filter((l) => l.trim().length > 0);
  if (lines.length === 0) return { snippet: 'Streaming...', isTool: false };

  const lastLine = lines[lines.length - 1].trim();
  if (lastLine.startsWith('🔧 call:')) {
    const match = lastLine.match(/🔧\s*call:\s*([a-zA-Z0-9_-]+)\((.*)\)/);
    if (match) {
      const argsPreview = match[2].length > 90 ? match[2].slice(0, 90) + '…' : match[2];
      return {
        snippet: `${match[1]}(${argsPreview})`,
        isTool: true,
        toolName: match[1],
      };
    }
    return { snippet: lastLine.slice(0, 160), isTool: true };
  }

  let clean = lastLine;
  if (clean.length > 170) {
    clean = '…' + clean.slice(-165);
  }
  return { snippet: clean, isTool: false };
}

const LiveStreamTile: React.FC<{
  stream: ActiveStreamState;
  now: number;
  onDismiss: (requestId: string) => void;
}> = ({ stream, now, onDismiss }) => {
  const ageMs = Math.max(0, now - stream.lastUpdated);
  const style = getDecayStyle(stream.isDone, ageMs);

  // Speed-linked border rotation speed: 100 tok/s -> 0.7s spin, 10 tok/s -> 3.5s spin
  const speed = stream.speedTokPerSec || 0;
  const spinDuration = speed > 0 ? Math.max(0.6, Math.min(3.8, 55 / speed)) : 2.5;

  const { snippet, isTool } = getTailSnippet(stream.text);

  return (
    <div
      className={cn(
        'relative rounded-xl p-[1px] overflow-hidden transition-all duration-500 font-mono text-xs',
        style.activeGlow
      )}
      style={{
        boxShadow: !stream.isDone ? `0 0 16px ${style.glow}20` : undefined,
      }}
    >
      {/* ── Spinning Border Beam (Speed-linked animation) ──────── */}
      {!stream.isDone && (
        <div
          className="absolute inset-[-150%] pointer-events-none"
          style={{
            background: `conic-gradient(from 0deg, transparent 0deg, ${style.glow} 40deg, transparent 80deg, transparent 360deg)`,
            animation: `border-spin ${spinDuration.toFixed(2)}s linear infinite`,
          }}
        />
      )}

      {/* ── Inner Compact Box (~80px total height) ─────────────── */}
      <div
        className="relative rounded-[11px] bg-[#02050c] p-2.5 flex flex-col justify-between h-[82px] transition-colors"
        style={{
          border: stream.isDone ? `1px solid ${style.border}` : '1px solid transparent',
        }}
      >
        {/* Row 1: Agent Identity & Metrics Header */}
        <div className="flex items-center justify-between gap-2">
          {/* Identity */}
          <div className="flex items-center gap-1.5 min-w-0">
            {stream.virtualModel ? (
              <span
                className={cn(
                  'px-1.5 py-0.5 rounded text-[10px] font-bold shrink-0 transition-colors',
                  style.badge
                )}
              >
                {stream.virtualModel}
              </span>
            ) : (
              <span className="px-1.5 py-0.5 rounded text-[10px] bg-slate-800/80 text-slate-400 border border-slate-700/50 shrink-0">
                direct
              </span>
            )}

            <div className="flex items-center gap-1 text-[11px] truncate">
              <span className={cn('font-semibold truncate transition-colors', style.providerText)}>
                {stream.provider}
              </span>
              <span className="text-slate-600">:</span>
              <span className={cn('truncate text-[10px] transition-colors', style.modelText)} title={stream.model}>
                {stream.model}
              </span>
            </div>
          </div>

          {/* Right Metrics: tok/s, tokens, status, dismiss */}
          <div className="flex items-center gap-2 shrink-0 text-[10px]">
            {/* Speed indicator */}
            {!stream.isDone ? (
              <span className="flex items-center gap-0.5 font-bold text-[#00f0ff]">
                <Zap className="w-2.5 h-2.5 fill-current animate-bounce" />
                {stream.speedTokPerSec}
                <span className="text-[9px] font-normal text-cyan-400/80">t/s</span>
              </span>
            ) : stream.speedTokPerSec > 0 ? (
              <span className={cn('flex items-center gap-0.5 font-medium', style.speedText)}>
                <Zap className="w-2.5 h-2.5" />
                {stream.speedTokPerSec}
                <span className="text-[9px] font-normal text-slate-500">t/s</span>
              </span>
            ) : null}

            {/* Token count */}
            <span className="text-amber-400/90 font-medium">
              {stream.tokensSoFar} <span className="text-[9px] font-normal text-slate-500">tok</span>
            </span>

            {/* Status / Relative age */}
            {stream.isDone ? (
              stream.error ? (
                <Badge variant="rose" className="text-[9px] px-1 py-0 h-4">
                  <AlertOctagon className="w-2.5 h-2.5" />
                  ERR
                </Badge>
              ) : (
                <span className="text-slate-500 text-[10px]">
                  {formatTimeAgo(stream.lastUpdated, now)}
                </span>
              )
            ) : (
              <span className="flex items-center gap-1 text-[#00f0ff] font-semibold text-[10px] animate-pulse">
                <span className="w-1.5 h-1.5 rounded-full bg-[#00f0ff] animate-ping" />
                LIVE
              </span>
            )}

            {/* Dismiss X */}
            <button
              onClick={() => onDismiss(stream.requestId)}
              className="p-0.5 rounded text-slate-500 hover:text-white hover:bg-white/[0.08] transition-colors"
              title="Dismiss agent tile"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        </div>

        {/* Row 2: Compact Live Ticker (Single line snippet / tool call) */}
        <div className="rounded-md bg-[#010308] border border-white/[0.04] px-2 py-1 flex items-center gap-1.5 overflow-hidden h-[28px]">
          {isTool ? (
            <div className="flex items-center gap-1 min-w-0 text-amber-300 font-medium text-[11px] truncate">
              <Wrench className="w-3 h-3 text-amber-400 shrink-0" />
              <span className="truncate">{snippet}</span>
            </div>
          ) : (
            <div className="flex items-center gap-1.5 min-w-0 w-full text-[11px]">
              <span className={cn('font-bold shrink-0 transition-colors', style.promptColor)}>
                &gt;
              </span>
              <span className={cn('truncate font-mono transition-colors', style.tickerText)}>
                {snippet}
              </span>
              {!stream.isDone && (
                <span className="inline-block w-1.5 h-3 bg-[#00f0ff] ml-0.5 shrink-0 animate-pulse" />
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export const LiveAgentDeck: React.FC<LiveAgentDeckProps> = ({
  streams,
  onDismiss,
  onClearCompleted,
}) => {
  const [now, setNow] = useState<number>(Date.now());
  const DECAY_WINDOW_MS = 4 * 60 * 1000; // Auto-prune cards older than 4 minutes

  // Smooth tick for relative times and decay colors
  useEffect(() => {
    const timer = setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  // 1. Filter: Retain active streams or completed streams within the 4-minute decay window
  const activeOrRecent = streams.filter(
    (s) => !s.isDone || now - s.lastUpdated <= DECAY_WINDOW_MS
  );

  // 2. Group by unique agent identity to show the latest state per agent
  const agentMap = new Map<string, ActiveStreamState>();
  for (const s of activeOrRecent) {
    const agentKey = s.virtualModel
      ? `${s.virtualModel}:${s.provider}`
      : `${s.provider}:${s.model}`;

    const existing = agentMap.get(agentKey);
    if (!existing) {
      agentMap.set(agentKey, s);
    } else {
      if (!s.isDone && existing.isDone) {
        agentMap.set(agentKey, s);
      } else if (s.isDone && !existing.isDone) {
        // keep active
      } else if (s.lastUpdated > existing.lastUpdated) {
        agentMap.set(agentKey, s);
      }
    }
  }

  // 3. Sort: Live active streams first, then most recently updated first
  const displayedAgents = Array.from(agentMap.values()).sort((a, b) => {
    if (!a.isDone && b.isDone) return -1;
    if (a.isDone && !b.isDone) return 1;
    return b.lastUpdated - a.lastUpdated;
  });

  const activeCount = displayedAgents.filter((s) => !s.isDone).length;
  const completedCount = displayedAgents.filter((s) => s.isDone).length;

  return (
    <div className="space-y-2.5 rounded-xl border border-white/[0.08] bg-[#02050e]/75 backdrop-blur p-3 font-mono shadow-[0_0_20px_rgba(0,0,0,0.5)]">
      {/* Compact Header */}
      <div className="flex items-center justify-between border-b border-white/[0.05] pb-2">
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-cyan-500/10 border border-cyan-500/30 text-cyan-400">
            <Terminal className="w-3.5 h-3.5" />
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs font-bold text-white tracking-wider">
              LIVE AGENT DECK
            </span>
            {activeCount > 0 && (
              <Badge variant="cyan" className="text-[10px] px-1.5 py-0 animate-pulse">
                {activeCount} ACTIVE
              </Badge>
            )}
            <span className="text-[11px] text-slate-500">
              ({displayedAgents.length} {displayedAgents.length === 1 ? 'agent' : 'agents'})
            </span>
          </div>
        </div>

        {/* Clear Action */}
        <div className="flex items-center gap-2">
          {completedCount > 0 && (
            <Button
              variant="secondary"
              size="sm"
              onClick={onClearCompleted}
              className="text-[10px] h-6 px-2 py-0 text-slate-400 hover:text-white"
            >
              Clear Completed
            </Button>
          )}
        </div>
      </div>

      {/* Grid of Compact ~80px Agent Tiles */}
      {displayedAgents.length === 0 ? (
        <div className="rounded-lg border border-white/[0.04] bg-[#010308] p-2.5 text-xs text-slate-500 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#00f0ff] opacity-40" />
              <span className="relative inline-flex rounded-full h-2 w-2 bg-[#00f0ff]/60" />
            </span>
            <span className="text-[11px]">
              Standing by on <span className="text-cyan-400 font-semibold">/ws/live</span>. Ready for active agent streams.
            </span>
          </div>
          <span className="text-[10px] text-slate-600">0 Active Streams</span>
        </div>
      ) : (
        <div className="grid gap-2.5 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
          {displayedAgents.map((stream) => (
            <LiveStreamTile
              key={`${stream.virtualModel || 'direct'}-${stream.provider}-${stream.requestId}`}
              stream={stream}
              now={now}
              onDismiss={onDismiss}
            />
          ))}
        </div>
      )}
    </div>
  );
};
