import React, { useEffect, useState } from 'react';
import { Terminal, AlertOctagon, X, Zap, Wrench } from 'lucide-react';
import { Badge, Button, cn } from '../ui';
import { useTheme } from '../theme';

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
  onClearAll?: () => void;
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

// Dynamic Theme Decay Calculator
function getThemeDecayStyle(primaryHex: string, primaryGlow: string, isDone: boolean, ageMs: number) {
  if (!isDone) {
    return {
      glow: primaryHex,
      border: primaryGlow,
      cardBg: 'bg-[var(--bg-card)]',
      badge: 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border-[var(--primary-bg-badge-border)] shadow-[0_0_10px_var(--primary-glow-subtle)]',
      providerText: 'text-white',
      modelText: 'text-slate-200',
      speedText: 'text-[var(--primary)]',
      promptColor: 'text-[var(--primary)]',
      tickerText: 'text-slate-100 font-medium',
      activeGlow: 'shadow-[0_0_20px_var(--primary-glow-subtle)]',
      cardOpacity: 'opacity-100',
    };
  }

  const sec = ageMs / 1000;
  if (sec < 25) {
    return {
      glow: primaryHex,
      border: 'rgba(var(--primary-rgb), 0.35)',
      cardBg: 'bg-[var(--bg-card)]',
      badge: 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border-[var(--primary-border)] shadow-sm',
      providerText: 'text-slate-100',
      modelText: 'text-slate-300',
      speedText: 'text-[var(--primary)]',
      promptColor: 'text-[var(--primary)]',
      tickerText: 'text-slate-200',
      activeGlow: 'shadow-[0_0_12px_var(--primary-glow-subtle)]',
      cardOpacity: 'opacity-100',
    };
  }
  if (sec < 75) {
    return {
      glow: primaryHex,
      border: 'rgba(var(--primary-rgb), 0.25)',
      cardBg: 'bg-[var(--bg-card)]',
      badge: 'bg-white/[0.05] text-slate-300 border-white/[0.08]',
      providerText: 'text-slate-200',
      modelText: 'text-slate-400',
      speedText: 'text-slate-300',
      promptColor: 'text-slate-400',
      tickerText: 'text-slate-300',
      activeGlow: '',
      cardOpacity: 'opacity-95',
    };
  }
  if (sec < 180) {
    return {
      glow: 'rgba(var(--primary-rgb), 0.5)',
      border: 'rgba(var(--primary-rgb), 0.15)',
      cardBg: 'bg-[var(--bg-card)]',
      badge: 'bg-white/[0.02] text-slate-400 border-white/[0.06]',
      providerText: 'text-slate-300',
      modelText: 'text-slate-500',
      speedText: 'text-slate-400',
      promptColor: 'text-slate-500',
      tickerText: 'text-slate-400',
      activeGlow: '',
      cardOpacity: 'opacity-85',
    };
  }
  return {
    glow: 'transparent',
    border: 'var(--border-subtle)',
    cardBg: 'bg-[var(--bg-card)]',
    badge: 'bg-white/[0.01] text-slate-500 border-white/[0.04]',
    providerText: 'text-slate-400',
    modelText: 'text-slate-600',
    speedText: 'text-slate-500',
    promptColor: 'text-slate-600',
    tickerText: 'text-slate-500',
    activeGlow: '',
    cardOpacity: 'opacity-70',
  };
}

// Extract last non-empty line / tool call, capped at ~180 chars
function getTailSnippet(text: string): { snippet: string; isTool: boolean; toolName?: string } {
  if (!text) return { snippet: 'Listening for upstream tokens...', isTool: false };

  // 1. Search for tool call anywhere in the text (even if arguments span multiple lines)
  const lastToolIdx = text.lastIndexOf('🔧 call:');
  if (lastToolIdx !== -1) {
    const toolSub = text.slice(lastToolIdx);
    const match = toolSub.match(/🔧\s*call:\s*([a-zA-Z0-9_-]+)\s*\(([\s\S]*?)(\)|$)/);
    if (match) {
      const toolName = match[1];
      const rawArgs = match[2].replace(/\s+/g, ' ').trim();
      const argsPreview = rawArgs.length > 80 ? rawArgs.slice(0, 80) + '…' : rawArgs;
      return {
        snippet: `${toolName}(${argsPreview})`,
        isTool: true,
        toolName,
      };
    }
    return { snippet: toolSub.slice(0, 160).replace(/\s+/g, ' '), isTool: true };
  }

  // 2. Check for incoming tool results or file operations
  if (text.includes('📥 Received tool result:')) {
    const lastResultIdx = text.lastIndexOf('📥 Received tool result:');
    const line = text.slice(lastResultIdx).split('\n')[0].trim();
    return { snippet: line, isTool: true };
  }

  // 3. Regular text fallback: get the last meaningful line
  const lines = text.trimEnd().split('\n').filter((l) => l.trim().length > 0);
  if (lines.length === 0) return { snippet: 'Streaming...', isTool: false };

  let lastLine = lines[lines.length - 1].trim();
  if (lastLine.length > 175) {
    lastLine = '…' + lastLine.slice(-170);
  }
  return { snippet: lastLine, isTool: false };
}

const LiveStreamTile: React.FC<{
  stream: ActiveStreamState;
  now: number;
  onDismiss: (requestId: string) => void;
}> = ({ stream, now, onDismiss }) => {
  const { currentConfig } = useTheme();
  const ageMs = Math.max(0, now - (stream.lastUpdated || 0));
  // A stream is only TRULY live if it has received a token in the last 10 seconds
  const isActuallyLive = !stream.isDone && ageMs <= 10_000;
  const style = getThemeDecayStyle(currentConfig.primary, currentConfig.glow, !isActuallyLive, ageMs);

  // Speed-linked border rotation speed: 100 tok/s -> 0.65s spin, 15 tok/s -> 3.2s spin
  const speed = stream.speedTokPerSec || 0;
  const spinDuration = speed > 0 ? Math.max(0.65, Math.min(3.6, 50 / speed)) : 2.2;

  const { snippet, isTool } = getTailSnippet(stream.text);

  return (
    <div
      className={cn(
        'relative rounded-xl p-[1px] overflow-hidden transition-all duration-500 font-mono text-xs',
        style.cardOpacity,
        style.activeGlow
      )}
      style={{
        boxShadow: isActuallyLive ? `0 0 20px ${style.glow}28` : undefined,
      }}
    >
      {/* ── Spinning Border Beam (Conic Theme Gradient linked to Speed) ── */}
      {isActuallyLive && (
        <div
          className="absolute inset-[-150%] pointer-events-none"
          style={{
            background: `conic-gradient(from 0deg, transparent 0deg, var(--primary) 35deg, var(--secondary) 75deg, transparent 115deg, transparent 360deg)`,
            animation: `border-spin ${spinDuration.toFixed(2)}s linear infinite`,
          }}
        />
      )}

      {/* ── Inner Compact Box (~82px height) ────────────────── */}
      <div
        className={cn(
          'relative rounded-[11px] p-2.5 flex flex-col justify-between h-[82px] transition-colors',
          style.cardBg
        )}
        style={{
          border: !isActuallyLive ? `1px solid ${style.border}` : '1px solid var(--primary-border)',
        }}
      >
        {/* Row 1: Agent Identity & Telemetry Metrics */}
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
              <span className="px-1.5 py-0.5 rounded text-[10px] bg-white/[0.06] text-slate-300 border border-white/[0.08] shrink-0">
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

          {/* Right Metrics: Speed, Tokens, Status, Dismiss */}
          <div className="flex items-center gap-2 shrink-0 text-[10px]">
            {/* Speed indicator */}
            {isActuallyLive ? (
              <span className="flex items-center gap-0.5 font-bold text-[var(--primary)] drop-shadow-[0_0_6px_var(--primary-glow)]">
                <Zap className="w-2.5 h-2.5 fill-current animate-bounce" />
                {stream.speedTokPerSec}
                <span className="text-[9px] font-normal opacity-80">t/s</span>
              </span>
            ) : stream.speedTokPerSec > 0 ? (
              <span className={cn('flex items-center gap-0.5 font-medium', style.speedText)}>
                <Zap className="w-2.5 h-2.5" />
                {stream.speedTokPerSec}
                <span className="text-[9px] font-normal text-slate-500">t/s</span>
              </span>
            ) : null}

            {/* Token count */}
            <span className="font-medium text-slate-200">
              {stream.tokensSoFar} <span className="text-[9px] font-normal text-slate-500">tok</span>
            </span>

            {/* Status / Relative age */}
            {!isActuallyLive ? (
              stream.error ? (
                <Badge variant="rose" className="text-[9px] px-1 py-0 h-4">
                  <AlertOctagon className="w-2.5 h-2.5" />
                  ERR
                </Badge>
              ) : (
                <span className="text-slate-400 text-[10px] font-mono">
                  {formatTimeAgo(stream.lastUpdated, now)}
                </span>
              )
            ) : (
              <span className="flex items-center gap-1 text-[var(--primary)] font-semibold text-[10px] animate-pulse bg-[var(--primary-bg-badge)] px-1.5 py-0.5 rounded border border-[var(--primary-border)]">
                <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] animate-ping" />
                LIVE
              </span>
            )}

            {/* Dismiss button */}
            <button
              onClick={() => onDismiss(stream.requestId)}
              className="p-0.5 rounded text-slate-500 hover:text-[var(--primary)] hover:bg-white/[0.08] transition-colors"
              title="Dismiss agent tile"
            >
              <X className="w-3 h-3" />
            </button>
          </div>
        </div>

        {/* Row 2: Compact Live Ticker (Single line snippet / tool call) */}
        <div className="rounded-md bg-[#01050e]/95 border border-[var(--border-subtle)] px-2 py-1 flex items-center gap-1.5 overflow-hidden h-[28px] shadow-inner">
          {isTool ? (
            <div className="flex items-center gap-1.5 min-w-0 font-medium text-[11px] truncate bg-[var(--primary-bg-badge)] px-1.5 py-0.5 rounded border border-[var(--primary-border)] text-[var(--primary)]">
              <Wrench className="w-3 h-3 shrink-0" />
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
                <span className="inline-block w-1.5 h-3 bg-[var(--primary)] ml-0.5 shrink-0 animate-pulse shadow-[0_0_8px_var(--primary)]" />
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
  onClearAll,
}) => {
  const [now, setNow] = useState<number>(Date.now());
  const DECAY_WINDOW_MS = 2 * 60 * 1000; // Auto-prune cards older than 2 minutes

  // Smooth tick for relative times and decay colors
  useEffect(() => {
    const timer = setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  // Helper: A stream is only truly live if it received tokens within the last 10 seconds
  const isActuallyLive = (s: ActiveStreamState) => !s.isDone && (now - (s.lastUpdated || 0) <= 10_000);

  // 1. Filter: Retain active streams or completed streams within the decay window
  const activeOrRecent = streams.filter(
    (s) => isActuallyLive(s) || (now - (s.lastUpdated || 0) <= DECAY_WINDOW_MS)
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
      const sLive = isActuallyLive(s);
      const existLive = isActuallyLive(existing);

      if (sLive && !existLive) {
        agentMap.set(agentKey, s);
      } else if (!sLive && existLive) {
        // keep active
      } else if (s.lastUpdated > existing.lastUpdated) {
        agentMap.set(agentKey, s);
      }
    }
  }

  // 3. Sort: Live active streams first, then most recently updated first
  const displayedAgents = Array.from(agentMap.values()).sort((a, b) => {
    const aLive = isActuallyLive(a);
    const bLive = isActuallyLive(b);
    if (aLive && !bLive) return -1;
    if (!aLive && bLive) return 1;
    return b.lastUpdated - a.lastUpdated;
  });

  const activeCount = displayedAgents.filter(isActuallyLive).length;
  const completedCount = displayedAgents.length - activeCount;

  return (
    <div className="space-y-2.5 rounded-2xl border border-[var(--primary-border)] bg-[var(--bg-card)] backdrop-blur-md p-3.5 font-mono shadow-[0_0_30px_var(--primary-glow-subtle)] transition-colors">
      {/* Header Bar */}
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] pb-2.5">
        <div className="flex items-center gap-2.5">
          <div className="p-1 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shadow-[0_0_12px_var(--primary-glow-subtle)]">
            <Terminal className="w-4 h-4" />
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs font-bold text-white tracking-wider flex items-center gap-1.5">
              LIVE AGENT DECK
              <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] inline-block shadow-[0_0_6px_var(--primary)]" />
            </span>
            {activeCount > 0 && (
              <span className="text-[10px] px-1.5 py-0.5 rounded font-mono bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)] animate-pulse">
                {activeCount} ACTIVE
              </span>
            )}
            <span className="text-[11px] text-slate-400 font-mono">
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
              className="text-[10px] h-6 px-2.5 py-0 hover:border-[var(--primary-border)] hover:text-[var(--primary)]"
            >
              Clear Completed
            </Button>
          )}
          {displayedAgents.length > 0 && onClearAll && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onClearAll}
              className="text-[10px] h-6 px-2 py-0 text-slate-400 hover:text-rose-400 hover:bg-rose-500/10"
              title="Clear all streams from the deck"
            >
              Clear All
            </Button>
          )}
        </div>
      </div>

      {/* Grid of Compact ~82px Agent Tiles */}
      {displayedAgents.length === 0 ? (
        <div className="rounded-xl border border-[var(--border-subtle)] bg-[#010814]/60 p-3 text-xs text-slate-400 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[var(--primary)] opacity-40" />
              <span className="relative inline-flex rounded-full h-2 w-2 bg-[var(--primary)]" />
            </span>
            <span className="text-[11px] text-slate-300">
              Standing by on <span className="text-[var(--primary)] font-semibold">/ws/live</span>. Ready for active agent streams.
            </span>
          </div>
          <span className="text-[10px] text-slate-500 font-mono">0 Active Streams</span>
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
