import React, { useEffect, useRef, useState } from 'react';
import { Terminal, CheckCircle2, AlertOctagon, X, Zap } from 'lucide-react';
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
  if (diffSec < 4) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.floor(diffMin / 60);
  return `${diffHr}h ago`;
}

const LiveStreamCard: React.FC<{
  stream: ActiveStreamState;
  now: number;
  onDismiss: (requestId: string) => void;
}> = ({ stream, now, onDismiss }) => {
  const terminalBoxRef = useRef<HTMLDivElement | null>(null);

  // Pure internal scroll: NEVER scroll the entire browser window
  useEffect(() => {
    if (terminalBoxRef.current) {
      terminalBoxRef.current.scrollTop = terminalBoxRef.current.scrollHeight;
    }
  }, [stream.text]);

  const shortRequestId = stream.requestId.length > 12
    ? stream.requestId.slice(-8)
    : stream.requestId;

  return (
    <div
      className={cn(
        'rounded-xl border transition-all duration-300 p-3.5 flex flex-col justify-between space-y-2.5 font-mono',
        stream.isDone
          ? 'bg-[#02050c]/90 border-white/[0.08] shadow-sm'
          : 'bg-[#020814] border-[#00f0ff]/50 shadow-[0_0_20px_rgba(0,240,255,0.12)]'
      )}
    >
      {/* Header bar */}
      <div className="flex items-center justify-between gap-2 border-b border-white/[0.06] pb-2">
        <div className="flex items-center gap-2 min-w-0">
          {stream.virtualModel ? (
            <span className="px-2 py-0.5 rounded text-[11px] font-bold font-mono bg-[#001c26] text-[#00f0ff] border border-[#00f0ff]/50 shadow-[0_0_8px_rgba(0,240,255,0.3)] shrink-0">
              {stream.virtualModel}
            </span>
          ) : (
            <span className="px-2 py-0.5 rounded text-[11px] font-mono bg-slate-800/80 text-slate-300 border border-slate-700 shrink-0">
              direct
            </span>
          )}

          <div className="flex items-center gap-1.5 text-xs truncate">
            <span className="text-white font-semibold truncate">{stream.provider}</span>
            <span className="text-slate-600">:</span>
            <span className="text-slate-300 truncate" title={stream.model}>
              {stream.model}
            </span>
          </div>
        </div>

        {/* Right action & badges */}
        <div className="flex items-center gap-2 shrink-0">
          {stream.isDone ? (
            stream.error ? (
              <Badge variant="rose">
                <AlertOctagon className="w-3 h-3 text-rose-400" />
                FAILED
              </Badge>
            ) : (
              <Badge variant="neutral">
                <CheckCircle2 className="w-3 h-3 text-slate-400" />
                DONE
              </Badge>
            )
          ) : (
            <Badge variant="cyan" className="animate-pulse">
              <span className="w-1.5 h-1.5 rounded-full bg-[#00f0ff] animate-ping" />
              STREAMING
            </Badge>
          )}

          <button
            onClick={() => onDismiss(stream.requestId)}
            className="p-1 rounded text-slate-500 hover:text-white hover:bg-white/[0.06] transition-colors"
            title="Dismiss stream"
          >
            <X className="w-3 h-3" />
          </button>
        </div>
      </div>

      {/* Telemetry Stats Bar: Displays speed, tokens, and stable relative update time */}
      <div className="flex items-center justify-between text-[11px] text-slate-400 px-1">
        <div className="flex items-center gap-3">
          {stream.speedTokPerSec > 0 && (
            <span className="flex items-center gap-1 text-[#00f0ff] font-semibold">
              <Zap className="w-3 h-3" />
              {stream.speedTokPerSec} <span className="text-[9px] font-normal text-slate-400">tok/s</span>
            </span>
          )}
          <span className="text-amber-400 font-medium">
            {stream.tokensSoFar} <span className="text-[9px] font-normal text-slate-400">tok</span>
          </span>
          <span className="text-slate-400">
            {stream.isDone ? (
              <span className="text-slate-400">
                Updated <span className="text-slate-300 font-medium">{formatTimeAgo(stream.lastUpdated, now)}</span>
              </span>
            ) : (
              <span className="text-cyan-300 font-medium">
                {(stream.elapsedMs / 1000).toFixed(1)}s <span className="text-[10px] text-slate-400 font-normal">(live)</span>
              </span>
            )}
          </span>
        </div>

        <div className="text-[10px] text-slate-500 font-mono" title={`Request ID: ${stream.requestId}`}>
          #{shortRequestId}
        </div>
      </div>

      {/* Live Stream Terminal Viewport - Internal scroll only */}
      <div
        ref={terminalBoxRef}
        className="relative rounded-lg bg-[#010309] border border-white/[0.06] p-2.5 max-h-36 min-h-[58px] overflow-y-auto text-xs text-cyan-100/90 whitespace-pre-wrap break-words leading-relaxed selection:bg-cyan-500/30 selection:text-white"
      >
        {stream.text ? (
          <>
            {stream.text}
            {!stream.isDone && (
              <span className="inline-block w-1.5 h-3.5 bg-[#00f0ff] ml-0.5 animate-pulse align-middle shadow-[0_0_8px_#00f0ff]" />
            )}
          </>
        ) : (
          <span className="text-slate-500 italic text-[11px]">
            {stream.isDone ? 'Empty response payload' : 'Intercepting upstream token stream...'}
          </span>
        )}
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

  // Update relative timestamps smoothly every second
  useEffect(() => {
    const timer = setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const activeStreams = streams.filter((s) => !s.isDone);
  const completedStreams = streams
    .filter((s) => s.isDone)
    .sort((a, b) => b.lastUpdated - a.lastUpdated);

  // Bound the deck strictly:
  // Show active streams first, plus at most enough completed streams to fill a single clean 2-card row (max 2 cards total)
  const maxCompleted = activeStreams.length >= 2 ? 0 : activeStreams.length === 1 ? 1 : 2;
  const boundedStreams = [
    ...activeStreams.slice(0, 2),
    ...completedStreams.slice(0, maxCompleted),
  ].slice(0, 2);

  const activeCount = activeStreams.length;
  const completedCount = completedStreams.length;

  return (
    <div className="space-y-3 rounded-2xl border border-[#00f0ff]/25 bg-[#020612]/80 backdrop-blur p-4 shadow-[0_0_30px_rgba(0,240,255,0.06)] font-mono transition-all">
      {/* Deck Header Bar */}
      <div className="flex items-center justify-between border-b border-white/[0.06] pb-3">
        <div className="flex items-center gap-2.5">
          <div className="p-1.5 rounded-lg bg-cyan-500/10 border border-cyan-500/30 text-cyan-400">
            <Terminal className="w-4 h-4" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-sm font-bold text-white tracking-wider font-mono">
                LIVE AGENT DECK
              </h3>
              {activeCount > 0 ? (
                <Badge variant="cyan" className="animate-pulse">
                  {activeCount} ACTIVE {activeCount === 1 ? 'STREAM' : 'STREAMS'}
                </Badge>
              ) : completedCount > 0 ? (
                <Badge variant="neutral">
                  IDLE &bull; {completedCount} RECENT
                </Badge>
              ) : (
                <Badge variant="neutral">
                  STANDBY
                </Badge>
              )}
            </div>
            <p className="text-[11px] text-slate-400 mt-0.5">
              Real-time token streaming tap intercepted directly from upstream inference models.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {completedCount > 0 && (
            <Button
              variant="secondary"
              size="sm"
              onClick={onClearCompleted}
              className="text-xs font-mono h-7"
            >
              Clear Completed ({completedCount})
            </Button>
          )}
        </div>
      </div>

      {/* Deck Body - Strictly bounded to at most 2 cards */}
      {boundedStreams.length === 0 ? (
        <div className="rounded-xl border border-white/[0.05] bg-[#01040a] p-3 text-xs text-slate-500 flex items-center justify-between">
          <div className="flex items-center gap-2.5">
            <span className="relative flex h-2 w-2">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#00f0ff] opacity-40" />
              <span className="relative inline-flex rounded-full h-2 w-2 bg-[#00f0ff]/60" />
            </span>
            <span>
              Connected to <span className="text-cyan-400 font-semibold">/ws/live</span>. Ready for incoming agent inference.
            </span>
          </div>
          <div className="text-[10px] text-slate-600">0 Active Streams</div>
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-3.5">
          {boundedStreams.map((stream) => (
            <LiveStreamCard
              key={stream.requestId}
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
