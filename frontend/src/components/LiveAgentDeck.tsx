import React, { useEffect, useRef } from 'react';
import { Terminal, Activity, CheckCircle2, AlertOctagon, X, Zap } from 'lucide-react';
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

const LiveStreamCard: React.FC<{
  stream: ActiveStreamState;
  onDismiss: (requestId: string) => void;
}> = ({ stream, onDismiss }) => {
  const terminalEndRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (terminalEndRef.current) {
      terminalEndRef.current.scrollIntoView({ behavior: 'smooth' });
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
          ? 'bg-[#02050c]/90 border-white/[0.08] shadow-sm opacity-90'
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

      {/* Telemetry Stats Bar */}
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
          <span>
            {(stream.elapsedMs / 1000).toFixed(1)}s
          </span>
        </div>

        <div className="text-[10px] text-slate-500" title={`Request ID: ${stream.requestId}`}>
          #{shortRequestId}
        </div>
      </div>

      {/* Live Stream Terminal Viewport */}
      <div className="relative rounded-lg bg-[#010309] border border-white/[0.06] p-2.5 max-h-36 min-h-[58px] overflow-y-auto text-xs text-cyan-100/90 whitespace-pre-wrap break-words leading-relaxed selection:bg-cyan-500/30 selection:text-white">
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
        <div ref={terminalEndRef} />
      </div>
    </div>
  );
};

export const LiveAgentDeck: React.FC<LiveAgentDeckProps> = ({
  streams,
  onDismiss,
  onClearCompleted,
}) => {
  const activeCount = streams.filter((s) => !s.isDone).length;
  const completedCount = streams.filter((s) => s.isDone).length;

  if (streams.length === 0) {
    return (
      <div className="flex items-center justify-between px-4 py-3 rounded-xl border border-white/[0.06] bg-[#030712]/70 backdrop-blur font-mono text-xs">
        <div className="flex items-center gap-2.5">
          <span className="relative flex h-2 w-2">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#00f0ff] opacity-40" />
            <span className="relative inline-flex rounded-full h-2 w-2 bg-[#00f0ff]/60" />
          </span>
          <span className="text-slate-400 font-semibold tracking-wider text-[11px]">
            LIVE AGENT DECK <span className="text-slate-600 font-normal">/ STANDBY</span>
          </span>
          <span className="text-slate-600 hidden sm:inline">&bull;</span>
          <span className="text-slate-500 text-[11px] hidden sm:inline">
            Telemetry socket connected on <span className="text-cyan-400/90">/ws/live</span>. Ready to stream live inference.
          </span>
        </div>
        <div className="flex items-center gap-2 text-[10px] text-slate-500 shrink-0">
          <span className="w-1.5 h-1.5 rounded-full bg-cyan-500/40" />
          0 Active Interceptions
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-3 rounded-2xl border border-[#00f0ff]/30 bg-[#020612]/80 backdrop-blur p-4 shadow-[0_0_30px_rgba(0,240,255,0.06)]">
      {/* Deck Header */}
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
              <Badge variant="cyan" className={cn(activeCount > 0 ? 'animate-pulse' : '')}>
                {activeCount} ACTIVE {activeCount === 1 ? 'STREAM' : 'STREAMS'}
              </Badge>
            </div>
            <p className="text-[11px] text-slate-400 mt-0.5 font-mono">
              Live token stream intercepted directly from upstream inference models.
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

      {/* Grid of active streams */}
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-3.5">
        {streams.map((stream) => (
          <LiveStreamCard key={stream.requestId} stream={stream} onDismiss={onDismiss} />
        ))}
      </div>
    </div>
  );
};
