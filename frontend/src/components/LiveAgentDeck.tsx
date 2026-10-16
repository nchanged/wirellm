import React, { useEffect, useState } from 'react';
import { AlertOctagon, X, Zap, Wrench, Cpu, Radio } from 'lucide-react';
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
  inFlightMap?: Record<string, number>;
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
      cardBg: 'bg-[#030916]/95',
      badge:
        'bg-[var(--primary-bg-badge)] text-[var(--primary)] border-[var(--primary-bg-badge-border)] shadow-[0_0_12px_var(--primary-glow-subtle)]',
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
      cardBg: 'bg-[#030812]/85',
      badge: 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border-[var(--primary-border)] shadow-sm',
      providerText: 'text-slate-100',
      modelText: 'text-slate-300',
      speedText: 'text-[var(--primary)]',
      promptColor: 'text-[var(--primary)]',
      tickerText: 'text-slate-200',
      activeGlow: 'shadow-[0_0_10px_var(--primary-glow-subtle)]',
      cardOpacity: 'opacity-100',
    };
  }
  if (sec < 75) {
    return {
      glow: primaryHex,
      border: 'rgba(var(--primary-rgb), 0.22)',
      cardBg: 'bg-[#030710]/80',
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
  return {
    glow: 'transparent',
    border: 'var(--border-subtle)',
    cardBg: 'bg-[#02050c]/75',
    badge: 'bg-white/[0.02] text-slate-400 border-white/[0.06]',
    providerText: 'text-slate-400',
    modelText: 'text-slate-500',
    speedText: 'text-slate-500',
    promptColor: 'text-slate-500',
    tickerText: 'text-slate-400',
    activeGlow: '',
    cardOpacity: 'opacity-80',
  };
}

// Extract last non-empty line / tool call, capped at ~160 chars
function getTailSnippet(text: string): { snippet: string; isTool: boolean; toolName?: string } {
  if (!text) return { snippet: 'Synchronizing neural uplink...', isTool: false };

  const lastToolIdx = text.lastIndexOf('🔧 call:');
  if (lastToolIdx !== -1) {
    const toolSub = text.slice(lastToolIdx);
    const match = toolSub.match(/🔧\s*call:\s*([a-zA-Z0-9_-]+)\s*\(([\s\S]*?)(\)|$)/);
    if (match) {
      const toolName = match[1];
      const rawArgs = match[2].replace(/\s+/g, ' ').trim();
      const argsPreview = rawArgs.length > 65 ? rawArgs.slice(0, 65) + '…' : rawArgs;
      return {
        snippet: `${toolName}(${argsPreview})`,
        isTool: true,
        toolName,
      };
    }
    return { snippet: toolSub.slice(0, 120).replace(/\s+/g, ' '), isTool: true };
  }

  if (text.includes('📥 Received tool result:')) {
    const lastResultIdx = text.lastIndexOf('📥 Received tool result:');
    const line = text.slice(lastResultIdx).split('\n')[0].trim();
    return { snippet: line, isTool: true };
  }

  const lines = text.trimEnd().split('\n').filter((l) => l.trim().length > 0);
  if (lines.length === 0) return { snippet: 'Streaming...', isTool: false };

  let lastLine = lines[lines.length - 1].trim();
  if (lastLine.length > 140) {
    lastLine = '…' + lastLine.slice(-135);
  }
  return { snippet: lastLine, isTool: false };
}

/**
 * Compact Mini Holographic SVG Core (36x36 emblem inside each compact bottom Agent Pod)
 */
const CompactAgentCoreSvg: React.FC<{
  isLive: boolean;
  isThinking: boolean;
  hasError: boolean;
  isTool: boolean;
  speed: number;
}> = ({ isLive, isThinking, hasError, isTool, speed }) => {
  const outerSpin = isLive ? Math.max(0.9, Math.min(4.5, 65 / Math.max(15, speed || 25))) : 16;
  const innerSpin = isLive ? Math.max(0.7, Math.min(3.2, 45 / Math.max(15, speed || 25))) : 11;
  const waveSpeed = speed > 0 ? Math.max(0.2, Math.min(0.75, 14 / speed)) : 0.8;

  const strokeColor = hasError
    ? '#f43f5e'
    : isLive
    ? 'var(--primary)'
    : 'rgba(148, 163, 184, 0.35)';
  const secondaryColor = hasError
    ? '#fb7185'
    : isLive
    ? 'var(--secondary)'
    : 'rgba(148, 163, 184, 0.2)';

  return (
    <div className="relative w-9 h-9 shrink-0 flex items-center justify-center select-none">
      <svg viewBox="0 0 64 64" className="w-9 h-9 overflow-visible">
        <circle
          cx="32"
          cy="32"
          r="26"
          fill={isLive ? 'rgba(var(--primary-rgb), 0.14)' : 'transparent'}
        />
        <g
          style={{
            transformOrigin: '32px 32px',
            animation: `border-spin ${outerSpin.toFixed(2)}s linear infinite`,
          }}
        >
          <circle
            cx="32"
            cy="32"
            r="24"
            fill="none"
            stroke={strokeColor}
            strokeWidth="2"
            strokeDasharray="18 6 4 6 28 8"
            strokeOpacity={isLive || hasError ? '0.95' : '0.35'}
          />
        </g>
        <g
          style={{
            transformOrigin: '32px 32px',
            animation: `border-spin ${innerSpin.toFixed(2)}s linear infinite reverse`,
          }}
        >
          <circle
            cx="32"
            cy="32"
            r="17"
            fill="none"
            stroke={secondaryColor}
            strokeWidth="1.8"
            strokeDasharray={isThinking ? '10 8' : '20 10 5 10'}
            strokeOpacity={isLive || hasError ? '0.9' : '0.3'}
          />
        </g>
        {hasError ? (
          <circle cx="32" cy="32" r="5" fill="#f43f5e" />
        ) : isLive && !isThinking ? (
          <g>
            {[
              { x: 25, minH: 6, maxH: 18, delay: '0s' },
              { x: 32, minH: 10, maxH: 26, delay: '0.15s' },
              { x: 39, minH: 6, maxH: 18, delay: '0.07s' },
            ].map((bar, i) => (
              <rect
                key={i}
                x={bar.x - 1.5}
                y={32 - bar.minH / 2}
                width="3"
                height={bar.minH}
                rx="1.5"
                fill={i === 1 ? '#ffffff' : 'var(--primary)'}
              >
                <animate
                  attributeName="height"
                  values={`${bar.minH};${bar.maxH};${bar.minH}`}
                  dur={`${waveSpeed.toFixed(2)}s`}
                  begin={bar.delay}
                  repeatCount="indefinite"
                />
                <animate
                  attributeName="y"
                  values={`${32 - bar.minH / 2};${32 - bar.maxH / 2};${32 - bar.minH / 2}`}
                  dur={`${waveSpeed.toFixed(2)}s`}
                  begin={bar.delay}
                  repeatCount="indefinite"
                />
              </rect>
            ))}
          </g>
        ) : isLive ? (
          <circle cx="32" cy="32" r="4.5" fill="var(--primary)">
            <animate attributeName="r" values="3;6;3" dur="1.1s" repeatCount="indefinite" />
          </circle>
        ) : (
          <circle cx="32" cy="32" r="3" fill={isTool ? secondaryColor : strokeColor} opacity="0.6" />
        )}
      </svg>
    </div>
  );
};

/**
 * FULL-WIDTH PANORAMIC SCI-FI HOLOGRAPHIC REACTOR STAGE (Top Full-Width Section, ~340px tall)
 * Massive glowing multi-wheel quantum turbine core in the center + sweeping plasma conduits
 * to orbital agent stations across the full width of the page.
 */
const NeuralReactorMatrixSvg: React.FC<{
  agents: ActiveStreamState[];
  isActuallyLive: (s: ActiveStreamState) => boolean;
  totalSpeed: number;
  activeCount: number;
  totalInFlight: number;
}> = ({ agents, isActuallyLive, totalSpeed, activeCount, totalInFlight }) => {
  const cx = 580;
  const cy = 175;
  const isReactorEngaged = activeCount > 0 || totalInFlight > 0;
  const effectiveActive = Math.max(activeCount, totalInFlight);

  // Wheel rotation speeds accelerate dramatically when agents are working or in-flight!
  const effectiveVelocity = totalSpeed > 0 ? totalSpeed : isReactorEngaged ? effectiveActive * 38 : 0;
  const outerWheelDur = isReactorEngaged
    ? Math.max(2.2, Math.min(8.0, 180 / Math.max(25, effectiveVelocity)))
    : 32;
  const turbineWheelDur = isReactorEngaged
    ? Math.max(1.1, Math.min(4.2, 95 / Math.max(25, effectiveVelocity)))
    : 22;
  const hexLatticeDur = isReactorEngaged
    ? Math.max(1.6, Math.min(5.5, 130 / Math.max(25, effectiveVelocity)))
    : 26;
  const gyroDur = isReactorEngaged
    ? Math.max(0.85, Math.min(2.6, 65 / Math.max(25, effectiveVelocity)))
    : 14;

  // 8 symmetric panoramic orbital docking coordinates across the 1160x350 canvas
  const orbitalSlots = [
    { x: 210, y: 78, side: 'left' as const },
    { x: 950, y: 78, side: 'right' as const },
    { x: 185, y: 268, side: 'left' as const },
    { x: 975, y: 268, side: 'right' as const },
    { x: 310, y: 175, side: 'left' as const },
    { x: 850, y: 175, side: 'right' as const },
    { x: 355, y: 62, side: 'left' as const },
    { x: 805, y: 62, side: 'right' as const },
  ];

  const satellites = agents.slice(0, 8).map((agent, idx) => {
    const slot = orbitalSlots[idx % orbitalSlots.length];
    const live = isActuallyLive(agent);
    const label = (agent.virtualModel || agent.provider || 'AGENT').toUpperCase();
    const sublabel = `${agent.provider}:${agent.model}`.slice(0, 28);
    const { snippet } = getTailSnippet(agent.text);
    const shortSnippet = snippet.length > 32 ? snippet.slice(0, 32) + '…' : snippet;
    return {
      agent,
      x: slot.x,
      y: slot.y,
      side: slot.side,
      live,
      label,
      sublabel,
      shortSnippet,
    };
  });

  // 24 radial harmonic spectrum bars around the inner reactor ring
  const spectrumBars = Array.from({ length: 24 }, (_, i) => {
    const angle = (i * 360) / 24;
    return { angle, delay: `${((i % 6) * 0.11).toFixed(2)}s` };
  });

  return (
    <div
      className={cn(
        'relative w-full rounded-xl border overflow-hidden flex flex-col justify-between p-3 sm:p-4 min-h-[320px] sm:min-h-[360px] select-none transition-all duration-500',
        isReactorEngaged
          ? 'border-[var(--primary)] bg-[#020714] shadow-[0_0_50px_var(--primary-glow-subtle),inset_0_0_60px_rgba(var(--primary-rgb),0.12)]'
          : 'border-white/10 bg-[#020611]/95 shadow-inner'
      )}
    >
      {/* Ambient Volumetric Sci-Fi Reactor Glow & Perspective Grid Backdrop */}
      <div
        className="absolute inset-0 pointer-events-none transition-opacity duration-500"
        style={{
          opacity: isReactorEngaged ? 0.95 : 0.4,
          backgroundImage: `
            radial-gradient(circle at 50% 50%, rgba(var(--primary-rgb), ${isReactorEngaged ? '0.28' : '0.10'}) 0%, rgba(var(--secondary-rgb), ${isReactorEngaged ? '0.12' : '0.04'}) 32%, transparent 68%),
            linear-gradient(to right, rgba(255,255,255,0.03) 1px, transparent 1px),
            linear-gradient(to bottom, rgba(255,255,255,0.03) 1px, transparent 1px)
          `,
          backgroundSize: '100% 100%, 36px 36px, 36px 36px',
        }}
      />

      {/* Top Tactical HUD Telemetry Strip */}
      <div className="relative z-10 flex items-center justify-between text-[10px] font-mono tracking-widest text-slate-400 px-1">
        <div className="flex items-center gap-3">
          <span className="flex items-center gap-1.5 text-[var(--primary)] font-bold drop-shadow-[0_0_8px_var(--primary-glow)]">
            <span
              className={cn(
                'w-2 h-2 rounded-full bg-[var(--primary)] shadow-[0_0_10px_var(--primary)]',
                isReactorEngaged ? 'animate-ping' : 'animate-pulse'
              )}
            />
            QUANTUM.REACTOR // JARVIS-MK9
          </span>
          <span className="hidden sm:inline text-slate-600">|</span>
          <span className="hidden sm:inline text-slate-400">
            TURBINE RPM:{' '}
            <strong className="text-white">
              {isReactorEngaged ? `${Math.round(1200 + effectiveVelocity * 45)} RPM` : '180 RPM (IDLE)'}
            </strong>
          </span>
        </div>

        <div className="flex items-center gap-3">
          <span
            className={cn(
              'px-2 py-0.5 rounded border text-[9px] font-bold tracking-wider',
              isReactorEngaged
                ? 'bg-[var(--primary-bg-badge)] border-[var(--primary)] text-[var(--primary)] shadow-[0_0_15px_var(--primary-glow)] animate-pulse'
                : 'bg-white/[0.03] border-white/10 text-slate-500'
            )}
          >
            {isReactorEngaged
              ? `⚡ REACTOR OVERDRIVE // ${effectiveActive} ACTIVE`
              : 'ORBITAL STANDBY'}
          </span>
        </div>
      </div>

      {/* Main Panoramic Full-Width SVG Stage (1160 x 330) */}
      <div className="relative z-10 flex-1 flex items-center justify-center my-1">
        <svg
          viewBox="0 0 1160 350"
          className="w-full h-[270px] sm:h-[315px] lg:h-[340px] overflow-visible"
          preserveAspectRatio="xMidYMid meet"
        >
          <defs>
            {/* Intense Sci-Fi Neon Bloom Filter */}
            <filter id="reactor-plasma-bloom" x="-40%" y="-40%" width="180%" height="180%">
              <feGaussianBlur stdDeviation="4.5" result="blur1" />
              <feGaussianBlur stdDeviation="11" result="blur2" />
              <feMerge>
                <feMergeNode in="blur2" />
                <feMergeNode in="blur1" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>

            <filter id="reactor-core-superglow" x="-60%" y="-60%" width="220%" height="220%">
              <feGaussianBlur stdDeviation="8" result="blur1" />
              <feGaussianBlur stdDeviation="20" result="blur2" />
              <feMerge>
                <feMergeNode in="blur2" />
                <feMergeNode in="blur1" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>

            <radialGradient id="central-plasma-grad" cx="50%" cy="50%" r="50%">
              <stop offset="0%" stopColor="#ffffff" stopOpacity={isReactorEngaged ? '0.9' : '0.25'} />
              <stop offset="28%" stopColor="var(--primary)" stopOpacity={isReactorEngaged ? '0.75' : '0.22'} />
              <stop offset="65%" stopColor="var(--secondary)" stopOpacity={isReactorEngaged ? '0.32' : '0.08'} />
              <stop offset="100%" stopColor="transparent" stopOpacity="0" />
            </radialGradient>

            <linearGradient id="panoramic-radar-sweep" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="var(--primary)" stopOpacity={isReactorEngaged ? '0.42' : '0.18'} />
              <stop offset="60%" stopColor="var(--secondary)" stopOpacity={isReactorEngaged ? '0.15' : '0.05'} />
              <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
            </linearGradient>

            <linearGradient id="conduit-grad-left" x1="100%" y1="50%" x2="0%" y2="50%">
              <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.95" />
              <stop offset="60%" stopColor="var(--secondary)" stopOpacity="0.65" />
              <stop offset="100%" stopColor="var(--primary)" stopOpacity="0.9" />
            </linearGradient>
          </defs>

          {/* ============================================================ */}
          {/* 1. PANORAMIC SCI-FI HORIZON GRID & SIDE TELEMETRY BRACKETS    */}
          {/* ============================================================ */}
          {/* Horizontal & Vertical Axis Crosshairs across full 1160px width */}
          <line
            x1="20"
            y1={cy}
            x2="1140"
            y2={cy}
            stroke="rgba(var(--primary-rgb), 0.16)"
            strokeWidth="1"
            strokeDasharray="6 6"
          />
          <line
            x1={cx}
            y1="8"
            x2={cx}
            y2="342"
            stroke="rgba(var(--primary-rgb), 0.16)"
            strokeWidth="1"
            strokeDasharray="6 6"
          />

          {/* Elliptical Orbital Field Rings spanning the wide stage */}
          <ellipse
            cx={cx}
            cy={cy}
            rx="440"
            ry="145"
            fill="none"
            stroke="rgba(var(--primary-rgb), 0.12)"
            strokeWidth="1"
            strokeDasharray="4 10"
          />
          <ellipse
            cx={cx}
            cy={cy}
            rx="310"
            ry="120"
            fill="none"
            stroke="rgba(var(--secondary-rgb), 0.14)"
            strokeWidth="1"
            strokeDasharray="12 8 2 8"
          />

          {/* Left & Right Flanking Sci-Fi HUD Framing Brackets & Sine Wave Telemetry */}
          <g opacity={isReactorEngaged ? '0.75' : '0.35'}>
            {/* Left HUD Frame */}
            <path
              d="M 28 40 L 14 54 L 14 296 L 28 310"
              fill="none"
              stroke="var(--primary)"
              strokeWidth="1.5"
            />
            <text x="26" y="34" fill="var(--primary)" fontSize="8.5" fontFamily="JetBrains Mono, monospace">
              SECTOR.ALPHA // PORT-WING
            </text>
            {/* Right HUD Frame */}
            <path
              d="M 1132 40 L 1146 54 L 1146 296 L 1132 310"
              fill="none"
              stroke="var(--primary)"
              strokeWidth="1.5"
            />
            <text
              x="1134"
              y="34"
              textAnchor="end"
              fill="var(--primary)"
              fontSize="8.5"
              fontFamily="JetBrains Mono, monospace"
            >
              SECTOR.OMEGA // STARBOARD-WING
            </text>
          </g>

          {/* ============================================================ */}
          {/* 2. SYNAPTIC PLASMA CONDUITS & ORBITAL AGENT SATELLITES       */}
          {/* ============================================================ */}
          {satellites.map((sat, idx) => {
            const dx = sat.x - cx;
            const dy = sat.y - cy;
            const ctrl1X = cx + dx * 0.38;
            const ctrl1Y = cy + dy * 0.12;
            const ctrl2X = cx + dx * 0.68;
            const ctrl2Y = sat.y;
            const pathD = `M ${cx} ${cy} C ${ctrl1X} ${ctrl1Y}, ${ctrl2X} ${ctrl2Y}, ${sat.x} ${sat.y}`;

            const packetDur =
              sat.agent.speedTokPerSec > 0
                ? Math.max(0.45, Math.min(1.6, 45 / sat.agent.speedTokPerSec))
                : 1.35;

            const nodeColor = sat.agent.error
              ? '#f43f5e'
              : sat.live
              ? 'var(--primary)'
              : 'rgba(148,163,184,0.35)';

            const cardW = 168;
            const cardH = 54;
            const cardX = sat.side === 'left' ? sat.x - cardW - 22 : sat.x + 22;
            const cardY = sat.y - cardH / 2;

            return (
              <g key={sat.agent.requestId || idx}>
                {/* Outer Glowing Plasma Sheath when LIVE */}
                {sat.live && (
                  <path
                    d={pathD}
                    fill="none"
                    stroke="var(--primary)"
                    strokeWidth="5"
                    strokeOpacity="0.22"
                    filter="url(#reactor-plasma-bloom)"
                  />
                )}

                {/* Base Synaptic Conduit */}
                <path
                  d={pathD}
                  fill="none"
                  stroke={nodeColor}
                  strokeWidth={sat.live ? '2.2' : '1.2'}
                  strokeOpacity={sat.live ? '0.7' : '0.28'}
                />

                {/* High-Velocity Laser Pulses & Photon Orbs when LIVE */}
                {sat.live && (
                  <>
                    <path
                      d={pathD}
                      fill="none"
                      stroke="#ffffff"
                      strokeWidth="2.4"
                      strokeDasharray="14 26"
                      filter="url(#reactor-plasma-bloom)"
                    >
                      <animate
                        attributeName="stroke-dashoffset"
                        from="80"
                        to="0"
                        dur={`${packetDur.toFixed(2)}s`}
                        repeatCount="indefinite"
                      />
                    </path>

                    {/* Primary Photon Packet */}
                    <circle r="4.5" fill="#ffffff" filter="url(#reactor-plasma-bloom)">
                      <animateMotion
                        dur={`${packetDur.toFixed(2)}s`}
                        repeatCount="indefinite"
                        path={pathD}
                      />
                    </circle>

                    {/* Secondary Trailing Photon Packet */}
                    <circle r="3" fill="var(--secondary)" filter="url(#reactor-plasma-bloom)">
                      <animateMotion
                        dur={`${packetDur.toFixed(2)}s`}
                        begin={`${(packetDur / 2).toFixed(2)}s`}
                        repeatCount="indefinite"
                        path={pathD}
                      />
                    </circle>
                  </>
                )}

                {/* Orbital Agent Satellite Node */}
                <g transform={`translate(${sat.x}, ${sat.y})`}>
                  {sat.live && (
                    <>
                      <circle r="14" fill="none" stroke="var(--primary)" strokeWidth="1.5">
                        <animate attributeName="r" values="12;28" dur="1.5s" repeatCount="indefinite" />
                        <animate attributeName="opacity" values="0.85;0" dur="1.5s" repeatCount="indefinite" />
                      </circle>
                      <circle
                        r="18"
                        fill="rgba(var(--primary-rgb), 0.2)"
                        filter="url(#reactor-plasma-bloom)"
                      />
                    </>
                  )}

                  {/* Counter-Rotating Satellite Lock-On Ring */}
                  <g
                    style={{
                      animation: sat.live
                        ? `border-spin ${Math.max(1.2, packetDur * 1.8).toFixed(2)}s linear infinite`
                        : 'border-spin 16s linear infinite',
                    }}
                  >
                    <circle
                      r="14"
                      fill="#030a18"
                      stroke={nodeColor}
                      strokeWidth="2"
                      strokeDasharray="10 4 3 4"
                    />
                  </g>

                  <circle
                    r="5.5"
                    fill={sat.agent.error ? '#f43f5e' : sat.live ? '#ffffff' : '#64748b'}
                    filter={sat.live ? 'url(#reactor-plasma-bloom)' : undefined}
                  />
                </g>

                {/* Connector Leader Line to Holographic Callout Plaque */}
                <line
                  x1={sat.side === 'left' ? sat.x - 14 : sat.x + 14}
                  y1={sat.y}
                  x2={sat.side === 'left' ? cardX + cardW : cardX}
                  y2={sat.y}
                  stroke={nodeColor}
                  strokeWidth="1.4"
                />

                {/* Floating Sci-Fi Holographic Callout Plaque inside SVG */}
                <g transform={`translate(${cardX}, ${cardY})`}>
                  <rect
                    x="0"
                    y="0"
                    width={cardW}
                    height={cardH}
                    rx="6"
                    fill="rgba(3, 10, 24, 0.92)"
                    stroke={nodeColor}
                    strokeWidth={sat.live ? '1.5' : '1'}
                  />
                  {/* Corner accent bar */}
                  <rect
                    x={sat.side === 'left' ? cardW - 3 : 0}
                    y="6"
                    width="3"
                    height={cardH - 12}
                    rx="1.5"
                    fill={sat.live ? 'var(--primary)' : '#475569'}
                  />
                  <text
                    x="10"
                    y="17"
                    fill={sat.live ? '#ffffff' : '#94a3b8'}
                    fontSize="11"
                    fontWeight="bold"
                    fontFamily="JetBrains Mono, monospace"
                  >
                    {sat.label}
                  </text>
                  <text
                    x={cardW - 10}
                    y="17"
                    textAnchor="end"
                    fill={sat.agent.error ? '#f43f5e' : sat.live ? 'var(--primary)' : '#64748b'}
                    fontSize="9.5"
                    fontWeight="bold"
                    fontFamily="JetBrains Mono, monospace"
                  >
                    {sat.agent.error
                      ? 'ERR'
                      : sat.live
                      ? sat.agent.speedTokPerSec > 0
                        ? `⚡ ${sat.agent.speedTokPerSec} t/s`
                        : '● WORKING'
                      : 'IDLE'}
                  </text>
                  <text
                    x="10"
                    y="31"
                    fill="#94a3b8"
                    fontSize="8.5"
                    fontFamily="JetBrains Mono, monospace"
                  >
                    {sat.sublabel}
                  </text>
                  <text
                    x="10"
                    y="45"
                    fill={sat.live ? 'var(--secondary)' : '#64748b'}
                    fontSize="8.5"
                    fontFamily="JetBrains Mono, monospace"
                  >
                    &gt; {sat.shortSnippet}
                  </text>
                </g>
              </g>
            );
          })}

          {/* ============================================================ */}
          {/* 3. MASSIVE CENTRAL QUANTUM ARC REACTOR CORE (r = 156px)      */}
          {/* ============================================================ */}
          {/* Volumetric Plasma Superglow Halo behind the Reactor */}
          <circle
            cx={cx}
            cy={cy}
            r={isReactorEngaged ? '165' : '125'}
            fill="url(#central-plasma-grad)"
            filter="url(#reactor-core-superglow)"
          />

          {/* Expanding Energy Shockwaves when Reactor is Engaged */}
          {isReactorEngaged && (
            <>
              <circle
                cx={cx}
                cy={cy}
                r="70"
                fill="none"
                stroke="var(--primary)"
                strokeWidth="2"
                filter="url(#reactor-plasma-bloom)"
              >
                <animate attributeName="r" values="62;162" dur="2.2s" repeatCount="indefinite" />
                <animate attributeName="opacity" values="0.85;0" dur="2.2s" repeatCount="indefinite" />
              </circle>
              <circle
                cx={cx}
                cy={cy}
                r="70"
                fill="none"
                stroke="var(--secondary)"
                strokeWidth="1.5"
              >
                <animate
                  attributeName="r"
                  values="62;162"
                  dur="2.2s"
                  begin="1.1s"
                  repeatCount="indefinite"
                />
                <animate
                  attributeName="opacity"
                  values="0.65;0"
                  dur="2.2s"
                  begin="1.1s"
                  repeatCount="indefinite"
                />
              </circle>
            </>
          )}

          {/* Sweeping Tactical Radar Beam */}
          <g
            style={{
              transformOrigin: `${cx}px ${cy}px`,
              animation: `border-spin ${isReactorEngaged ? '3.2s' : '12s'} linear infinite`,
            }}
          >
            <path
              d={`M ${cx} ${cy} L ${cx} ${cy - 148} A 148 148 0 0 1 ${cx + 115} ${cy - 93} Z`}
              fill="url(#panoramic-radar-sweep)"
            />
          </g>

          {/* WHEEL 1: Outer Compass & Degree Azimuth Bezel (r = 152) */}
          <g
            style={{
              transformOrigin: `${cx}px ${cy}px`,
              animation: `border-spin ${outerWheelDur.toFixed(2)}s linear infinite`,
            }}
          >
            <circle
              cx={cx}
              cy={cy}
              r="152"
              fill="none"
              stroke="rgba(var(--primary-rgb), 0.35)"
              strokeWidth="1.5"
              strokeDasharray="3 9.26"
            />
            <circle
              cx={cx}
              cy={cy}
              r="144"
              fill="none"
              stroke="var(--primary)"
              strokeWidth="1.8"
              strokeDasharray="55 25 12 25 85 30"
              strokeOpacity={isReactorEngaged ? '0.9' : '0.4'}
            />
            {/* Cardinal Lock Triangles */}
            <polygon points={`${cx},${cy - 158} ${cx - 5},${cy - 148} ${cx + 5},${cy - 148}`} fill="var(--primary)" />
            <polygon points={`${cx},${cy + 158} ${cx - 5},${cy + 148} ${cx + 5},${cy + 148}`} fill="var(--primary)" />
            <polygon points={`${cx - 158},${cy} ${cx - 148},${cy - 5} ${cx - 148},${cy + 5}`} fill="var(--secondary)" />
            <polygon points={`${cx + 158},${cy} ${cx + 148},${cy - 5} ${cx + 148},${cy + 5}`} fill="var(--secondary)" />
          </g>

          {/* WHEEL 2: Heavy Segmented Tokamak Turbine Wheel (r = 124) — Spins fast with Neon Glow when Active! */}
          <g
            style={{
              transformOrigin: `${cx}px ${cy}px`,
              animation: `border-spin ${turbineWheelDur.toFixed(2)}s linear infinite reverse`,
            }}
          >
            <circle
              cx={cx}
              cy={cy}
              r="124"
              fill="none"
              stroke="var(--primary)"
              strokeWidth={isReactorEngaged ? '7.5' : '4.5'}
              strokeDasharray="26 14 10 14 44 18"
              strokeOpacity={isReactorEngaged ? '0.95' : '0.38'}
              filter={isReactorEngaged ? 'url(#reactor-plasma-bloom)' : undefined}
            />
            <circle
              cx={cx}
              cy={cy}
              r="114"
              fill="none"
              stroke="var(--secondary)"
              strokeWidth="2"
              strokeDasharray="8 16"
              strokeOpacity={isReactorEngaged ? '0.85' : '0.3'}
            />
          </g>

          {/* WHEEL 3: Counter-Rotating Cryo-Magnetic Hexagonal Lattice (r = 96) */}
          <g
            style={{
              transformOrigin: `${cx}px ${cy}px`,
              animation: `border-spin ${hexLatticeDur.toFixed(2)}s linear infinite`,
            }}
          >
            <circle
              cx={cx}
              cy={cy}
              r="96"
              fill="none"
              stroke="var(--secondary)"
              strokeWidth="2.2"
              strokeDasharray="36 14 8 14"
              strokeOpacity={isReactorEngaged ? '0.9' : '0.35'}
              filter={isReactorEngaged ? 'url(#reactor-plasma-bloom)' : undefined}
            />
            {/* Octagonal Inscribed Geometric Web */}
            <polygon
              points={`${cx},${cy - 94} ${cx + 66},${cy - 66} ${cx + 94},${cy} ${cx + 66},${cy + 66} ${cx},${cy + 94} ${cx - 66},${cy + 66} ${cx - 94},${cy} ${cx - 66},${cy - 66}`}
              fill="none"
              stroke="rgba(var(--primary-rgb), 0.35)"
              strokeWidth="1.2"
            />
          </g>

          {/* WHEEL 4: 3D Tilted Quantum Gyroscope Ellipses (rx = 78, ry = 30) */}
          <g
            style={{
              transformOrigin: `${cx}px ${cy}px`,
              animation: `border-spin ${gyroDur.toFixed(2)}s linear infinite`,
            }}
          >
            <ellipse
              cx={cx}
              cy={cy}
              rx="78"
              ry="30"
              fill="none"
              stroke="var(--primary)"
              strokeWidth="2"
              strokeOpacity={isReactorEngaged ? '0.88' : '0.3'}
              transform={`rotate(35 ${cx} ${cy})`}
              filter={isReactorEngaged ? 'url(#reactor-plasma-bloom)' : undefined}
            />
          </g>
          <g
            style={{
              transformOrigin: `${cx}px ${cy}px`,
              animation: `border-spin ${(gyroDur * 1.15).toFixed(2)}s linear infinite reverse`,
            }}
          >
            <ellipse
              cx={cx}
              cy={cy}
              rx="78"
              ry="30"
              fill="none"
              stroke="var(--secondary)"
              strokeWidth="2"
              strokeOpacity={isReactorEngaged ? '0.88' : '0.3'}
              transform={`rotate(-35 ${cx} ${cy})`}
              filter={isReactorEngaged ? 'url(#reactor-plasma-bloom)' : undefined}
            />
          </g>

          {/* WHEEL 5: Circular Harmonic Equalizer Spikes around Inner Singularity (r = 56..72) */}
          <g>
            {spectrumBars.map((bar, idx) => (
              <g key={idx} transform={`rotate(${bar.angle} ${cx} ${cy})`}>
                <rect
                  x={cx - 1.2}
                  y={cy - 64}
                  width="2.4"
                  height={isReactorEngaged ? '10' : '4'}
                  rx="1"
                  fill={idx % 2 === 0 ? 'var(--primary)' : 'var(--secondary)'}
                  opacity={isReactorEngaged ? '0.95' : '0.35'}
                >
                  {isReactorEngaged && (
                    <animate
                      attributeName="height"
                      values="5;18;5"
                      dur={`${Math.max(0.35, gyroDur * 0.45).toFixed(2)}s`}
                      begin={bar.delay}
                      repeatCount="indefinite"
                    />
                  )}
                </rect>
              </g>
            ))}
          </g>

          {/* INNER REACTOR SINGULARITY SHIELD & CORE READOUT */}
          <circle
            cx={cx}
            cy={cy}
            r="50"
            fill="#020816"
            stroke="var(--primary)"
            strokeWidth="2.5"
            filter={isReactorEngaged ? 'url(#reactor-plasma-bloom)' : undefined}
          />
          <polygon
            points={`${cx},${cy - 44} ${cx + 38},${cy - 22} ${cx + 38},${cy + 22} ${cx},${cy + 44} ${cx - 38},${cy + 22} ${cx - 38},${cy - 22}`}
            fill="rgba(var(--primary-rgb), 0.12)"
            stroke="var(--secondary)"
            strokeWidth="1.5"
          />

          {/* Center Core Live Readout */}
          <text
            x={cx}
            y={cy - 14}
            textAnchor="middle"
            fill="var(--primary)"
            fontSize="8.5"
            fontWeight="bold"
            fontFamily="JetBrains Mono, monospace"
            letterSpacing="0.14em"
          >
            {isReactorEngaged ? 'OVERDRIVE' : 'STANDBY'}
          </text>
          <text
            x={cx}
            y={cy + 8}
            textAnchor="middle"
            fill="#ffffff"
            fontSize="22"
            fontWeight="bold"
            fontFamily="JetBrains Mono, monospace"
            filter={isReactorEngaged ? 'url(#reactor-plasma-bloom)' : undefined}
          >
            {totalSpeed > 0
              ? `${totalSpeed}`
              : isReactorEngaged
              ? `${effectiveActive} LIVE`
              : 'IDLE'}
          </text>
          <text
            x={cx}
            y={cy + 24}
            textAnchor="middle"
            fill="var(--secondary)"
            fontSize="8.5"
            fontWeight="bold"
            fontFamily="JetBrains Mono, monospace"
            letterSpacing="0.1em"
          >
            {totalSpeed > 0
              ? 'TOK / SEC'
              : isReactorEngaged
              ? 'NEURAL SYNC'
              : 'CORE MATRIX'}
          </text>
        </svg>
      </div>

      {/* Bottom Reactor Chamber Telemetry Bar */}
      <div className="relative z-10 flex flex-wrap items-center justify-between gap-2 text-[10px] font-mono text-slate-400 border-t border-white/[0.08] pt-2 px-1">
        <div className="flex items-center gap-4">
          <span>
            ORBITAL NODES: <strong className="text-white">{agents.length}</strong>
          </span>
          <span>
            IN-FLIGHT STREAMS: <strong className="text-[var(--primary)]">{effectiveActive}</strong>
          </span>
        </div>
        <span className="text-[var(--secondary)] font-semibold">
          {totalSpeed > 0
            ? `⚡ ${totalSpeed} TOK/S AGGREGATE THROUGHPUT`
            : isReactorEngaged
            ? '⚡ SYNAPTIC CONDUITS ENGAGED — PROCESSING UPSTREAM'
            : 'READY // LISTENING ON /WS/LIVE'}
        </span>
      </div>
    </div>
  );
};

/**
 * Compact Sci-Fi Agent HUD Pod (Placed in a 3-to-4 column grid BELOW the giant full-width Reactor)
 */
const SciFiAgentPod: React.FC<{
  stream: ActiveStreamState;
  isActuallyLive: boolean;
  now: number;
  onDismiss: (requestId: string) => void;
}> = ({ stream, isActuallyLive, now, onDismiss }) => {
  const { currentConfig } = useTheme();
  const ageMs = Math.max(0, now - (stream.lastUpdated || 0));
  const style = getThemeDecayStyle(currentConfig.primary, currentConfig.glow, !isActuallyLive, ageMs);

  const speed = stream.speedTokPerSec || 0;
  const spinDuration = speed > 0 ? Math.max(0.75, Math.min(3.4, 50 / speed)) : 2.0;
  const { snippet, isTool } = getTailSnippet(stream.text);
  const isThinking =
    isActuallyLive &&
    (speed === 0 || snippet.toLowerCase().includes('thinking') || snippet.toLowerCase().includes('processing'));

  return (
    <div
      className={cn(
        'relative rounded-lg p-[1px] overflow-hidden transition-all duration-300 font-mono text-xs',
        style.cardOpacity,
        style.activeGlow
      )}
      style={{
        boxShadow: isActuallyLive ? `0 0 18px ${style.glow}24` : undefined,
      }}
    >
      {/* Spinning Conic Laser Beam when LIVE */}
      {isActuallyLive && (
        <div
          className="absolute inset-[-150%] pointer-events-none"
          style={{
            background: `conic-gradient(from 0deg, transparent 0deg, var(--primary) 35deg, var(--secondary) 75deg, transparent 115deg, transparent 360deg)`,
            animation: `border-spin ${spinDuration.toFixed(2)}s linear infinite`,
          }}
        />
      )}

      {/* Compact Inner Pod (~64px height) */}
      <div
        className={cn(
          'relative rounded-[7px] px-2.5 py-2 flex items-center gap-2 transition-colors overflow-hidden',
          style.cardBg
        )}
        style={{
          border: !isActuallyLive ? `1px solid ${style.border}` : '1px solid var(--primary-border)',
        }}
      >
        {/* Compact 36x36 Animated SVG Emblem */}
        <CompactAgentCoreSvg
          isLive={isActuallyLive}
          isThinking={isThinking}
          hasError={Boolean(stream.error)}
          isTool={isTool}
          speed={speed}
        />

        {/* Right Compact Telemetry & Single-Line Ticker */}
        <div className="flex-1 min-w-0 space-y-1">
          {/* Row 1: Agent Callsign + Metrics */}
          <div className="flex items-center justify-between gap-1.5">
            <div className="flex items-center gap-1.5 min-w-0">
              {stream.virtualModel ? (
                <span
                  className={cn(
                    'px-1.5 py-0.2 rounded text-[9px] font-bold shrink-0 uppercase tracking-wider',
                    style.badge
                  )}
                >
                  {stream.virtualModel}
                </span>
              ) : (
                <span className="px-1.5 py-0.2 rounded text-[9px] bg-white/[0.06] text-slate-300 border border-white/[0.08] shrink-0 uppercase">
                  direct
                </span>
              )}

              <span className={cn('font-semibold text-[10px] truncate', style.providerText)}>
                {stream.provider}
              </span>
              <span className="text-slate-600 text-[10px]">:</span>
              <span className={cn('truncate text-[9px]', style.modelText)} title={stream.model}>
                {stream.model}
              </span>
            </div>

            <div className="flex items-center gap-1.5 shrink-0 text-[9px]">
              {isActuallyLive && speed > 0 && (
                <span className="flex items-center gap-0.5 font-bold text-[var(--primary)]">
                  <Zap className="w-2.5 h-2.5 fill-current" />
                  {speed}t/s
                </span>
              )}

              <span className="text-slate-300 font-medium">
                {stream.tokensSoFar.toLocaleString()}
                <span className="text-slate-500 ml-0.5">tok</span>
              </span>

              {!isActuallyLive ? (
                stream.error ? (
                  <Badge variant="rose" className="text-[8px] px-1 py-0 h-3.5">
                    ERR
                  </Badge>
                ) : (
                  <span className="text-slate-500 text-[9px]">
                    {formatTimeAgo(stream.lastUpdated, now)}
                  </span>
                )
              ) : (
                <span className="flex items-center gap-1 text-[var(--primary)] font-bold text-[8px] bg-[var(--primary-bg-badge)] px-1 py-0.2 rounded border border-[var(--primary-border)]">
                  <span className="w-1 h-1 rounded-full bg-[var(--primary)] animate-ping" />
                  {isThinking && speed === 0 ? 'WORK' : 'LIVE'}
                </span>
              )}

              <button
                onClick={() => onDismiss(stream.requestId)}
                className="p-0.5 rounded text-slate-500 hover:text-[var(--primary)] hover:bg-white/[0.08] transition-colors"
                title="Dismiss agent pod"
              >
                <X className="w-2.5 h-2.5" />
              </button>
            </div>
          </div>

          {/* Row 2: Compact Single-Line Visor Strip */}
          <div className="rounded bg-[#01050e]/95 border border-[var(--border-subtle)] px-2 py-0.5 flex items-center gap-1.5 overflow-hidden h-[22px]">
            {isTool ? (
              <div className="flex items-center gap-1 min-w-0 font-medium text-[10px] truncate text-[var(--primary)]">
                <Wrench className="w-2.5 h-2.5 shrink-0" />
                <span className="truncate">{snippet}</span>
              </div>
            ) : (
              <div className="flex items-center gap-1 min-w-0 w-full text-[10px]">
                <span className={cn('font-bold shrink-0', style.promptColor)}>&gt;</span>
                <span className={cn('truncate font-mono', style.tickerText)}>{snippet}</span>
                {isActuallyLive && (
                  <span className="inline-block w-1 h-2.5 bg-[var(--primary)] ml-0.5 shrink-0 animate-pulse" />
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export const LiveAgentDeck: React.FC<LiveAgentDeckProps> = ({
  streams,
  inFlightMap,
  onDismiss,
  onClearCompleted,
  onClearAll,
}) => {
  const [now, setNow] = useState<number>(Date.now());
  const DECAY_WINDOW_MS = 3 * 60 * 1000;

  useEffect(() => {
    const timer = setInterval(() => {
      setNow(Date.now());
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  // Total in-flight count across all providers
  const totalInFlight = Object.values(inFlightMap || {}).reduce((acc, v) => acc + (v || 0), 0);

  // A stream is live if it received tokens within the last 12s OR if its upstream provider still reports in_flight > 0
  const isActuallyLive = (s: ActiveStreamState) => {
    const age = now - (s.lastUpdated || 0);
    if ((inFlightMap?.[s.provider] ?? 0) > 0 && age <= 180_000 && !s.error) {
      return true;
    }
    if (s.isDone) return false;
    return age <= 12_000;
  };

  const activeOrRecent = streams.filter(
    (s) => isActuallyLive(s) || now - (s.lastUpdated || 0) <= DECAY_WINDOW_MS
  );

  // Group by unique agent identity to show the latest state per agent
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

  const displayedAgents = Array.from(agentMap.values()).sort((a, b) => {
    const aLive = isActuallyLive(a);
    const bLive = isActuallyLive(b);
    if (aLive && !bLive) return -1;
    if (!aLive && bLive) return 1;
    return b.lastUpdated - a.lastUpdated;
  });

  const liveAgents = displayedAgents.filter(isActuallyLive);
  const activeCount = liveAgents.length;
  const completedCount = displayedAgents.length - activeCount;
  const totalSpeed = liveAgents.reduce((acc, s) => acc + (s.speedTokPerSec || 0), 0);

  return (
    <div className="space-y-3 rounded-2xl border border-[var(--primary-border)] bg-[var(--bg-card)] backdrop-blur-md p-4 font-mono shadow-[0_0_40px_var(--primary-glow-subtle)] transition-colors relative overflow-hidden">
      {/* Top Sci-Fi Command Header Bar */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-subtle)] pb-2.5">
        <div className="flex items-center gap-2.5">
          <div className="p-1.5 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shadow-[0_0_14px_var(--primary-glow-subtle)]">
            <Cpu className="w-4 h-4 animate-pulse" />
          </div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-xs font-bold text-white tracking-wider flex items-center gap-1.5">
              NEURAL REACTOR MATRIX
              <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] inline-block shadow-[0_0_8px_var(--primary)] animate-ping" />
            </span>
            {activeCount > 0 || totalInFlight > 0 ? (
              <span className="text-[10px] px-2 py-0.5 rounded font-mono bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)] font-bold shadow-[0_0_12px_var(--primary-glow)] animate-pulse">
                {Math.max(activeCount, totalInFlight)} SYNAPSE ACTIVE
              </span>
            ) : (
              <span className="text-[10px] px-2 py-0.5 rounded font-mono bg-white/[0.04] text-slate-400 border border-white/10">
                STANDBY HUD
              </span>
            )}
            {totalSpeed > 0 && (
              <span className="text-[10px] px-2 py-0.5 rounded font-mono bg-[var(--secondary-bg-badge)] text-[var(--secondary)] border border-[var(--secondary-border)] font-bold">
                ⚡ {totalSpeed} t/s
              </span>
            )}
            <span className="text-[11px] text-slate-400 font-mono">
              ({displayedAgents.length} {displayedAgents.length === 1 ? 'node' : 'nodes'})
            </span>
          </div>
        </div>

        {/* Clear Actions */}
        <div className="flex items-center gap-2">
          {completedCount > 0 && (
            <Button
              variant="secondary"
              size="sm"
              onClick={onClearCompleted}
              className="text-[10px] h-6 px-2.5 py-0 hover:border-[var(--primary-border)] hover:text-[var(--primary)]"
            >
              Purge Idle ({completedCount})
            </Button>
          )}
          {displayedAgents.length > 0 && onClearAll && (
            <Button
              variant="ghost"
              size="sm"
              onClick={onClearAll}
              className="text-[10px] h-6 px-2 py-0 text-slate-400 hover:text-rose-400 hover:bg-rose-500/10"
              title="Clear all neural nodes from the matrix"
            >
              Clear Matrix
            </Button>
          )}
        </div>
      </div>

      {/* 1. TOP FULL-WIDTH TALL HOLOGRAPHIC REACTOR CORE STAGE */}
      <NeuralReactorMatrixSvg
        agents={displayedAgents}
        isActuallyLive={isActuallyLive}
        totalSpeed={totalSpeed}
        activeCount={activeCount}
        totalInFlight={totalInFlight}
      />

      {/* 2. BOTTOM COMPACT AGENT PODS GRID (Moved below the reactor, smaller & sleeker) */}
      {displayedAgents.length > 0 && (
        <div className="grid gap-2 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 pt-0.5">
          {displayedAgents.map((stream) => (
            <SciFiAgentPod
              key={`${stream.virtualModel || 'direct'}-${stream.provider}-${stream.requestId}`}
              stream={stream}
              isActuallyLive={isActuallyLive(stream)}
              now={now}
              onDismiss={onDismiss}
            />
          ))}
        </div>
      )}
    </div>
  );
};
