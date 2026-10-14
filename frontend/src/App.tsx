import React, { useEffect, useRef, useState } from 'react';
import { ApiStatusResponse, LiveStreamEvent } from './types';
import { fetchStatus } from './api';
import { Overview } from './components/Overview';
import { ProvidersStudio } from './components/ProvidersStudio';
import { VirtualModelsStudio } from './components/VirtualModelsStudio';
import { AnalyticsView } from './components/AnalyticsView';
import { LogsView } from './components/LogsView';
import { ThemeSelector } from './components/ThemeSelector';
import {
  Workflow,
  Server,
  SlidersHorizontal,
  BarChart3,
  Terminal,
  Radio,
  RefreshCw,
  Cpu,
} from 'lucide-react';
import { Badge, Button, cn } from './ui';

type Tab = 'overview' | 'providers' | 'models' | 'analytics' | 'logs';

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<Tab>('overview');
  const [status, setStatus] = useState<ApiStatusResponse | null>(null);
  const [isConnected, setIsConnected] = useState<boolean>(true);
  const [isRefreshing, setIsRefreshing] = useState<boolean>(false);
  const [wsConnected, setWsConnected] = useState<boolean>(false);
  const [latestLiveEvent, setLatestLiveEvent] = useState<LiveStreamEvent | null>(null);
  const [recentStreams, setRecentStreams] = useState<any[] | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  const loadStatus = async () => {
    try {
      setIsRefreshing(true);
      const res = await fetchStatus();
      setStatus(res);
      setIsConnected(true);
    } catch (err) {
      console.error(err);
      setIsConnected(false);
    } finally {
      setIsRefreshing(false);
    }
  };

  // Setup WebSocket connection to /ws/live
  useEffect(() => {
    let unmounted = false;
    let reconnectTimeout: any = null;

    const connectWs = () => {
      if (unmounted) return;
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const host = window.location.host || 'localhost:8765';
      const wsUrl = `${protocol}//${host}/ws/live`;

      try {
        const ws = new WebSocket(wsUrl);
        wsRef.current = ws;

        ws.onopen = () => {
          if (unmounted) return;
          setWsConnected(true);
          setIsConnected(true);
        };

        ws.onmessage = (event) => {
          if (unmounted) return;
          try {
            const parsed = JSON.parse(event.data);
            if (parsed.type === 'status' && parsed.data) {
              setStatus(parsed.data);
              setIsConnected(true);
            } else if (parsed.type === 'token' && parsed.data) {
              setLatestLiveEvent(parsed.data);
            } else if (parsed.type === 'recent_streams' && Array.isArray(parsed.data)) {
              setRecentStreams(parsed.data);
            }
          } catch {
            // Ignore non-json frames
          }
        };

        ws.onclose = () => {
          if (unmounted) return;
          setWsConnected(false);
          wsRef.current = null;
          // Reconnect with 2s backoff
          reconnectTimeout = setTimeout(connectWs, 2000);
        };

        ws.onerror = () => {
          if (unmounted) return;
          setWsConnected(false);
        };
      } catch (err) {
        setWsConnected(false);
        reconnectTimeout = setTimeout(connectWs, 3000);
      }
    };

    // Initial HTTP fetch to ensure instant display even before WS handshake
    loadStatus();
    connectWs();

    // Fallback polling only when WebSocket is NOT connected
    const fallbackInterval = setInterval(() => {
      if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) {
        loadStatus();
      }
    }, 4000);

    return () => {
      unmounted = true;
      if (reconnectTimeout) clearTimeout(reconnectTimeout);
      clearInterval(fallbackInterval);
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, []);

  const handleManualRefresh = () => {
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({ action: 'refresh' }));
    }
    loadStatus();
  };

  const navItems = [
    { id: 'overview' as Tab, label: 'Routing Matrix', short: 'Router', icon: Workflow },
    { id: 'providers' as Tab, label: 'Providers & Vault', short: 'Providers', icon: Server },
    { id: 'models' as Tab, label: 'Virtual Models', short: 'Models', icon: SlidersHorizontal },
    { id: 'analytics' as Tab, label: 'Token Analytics', short: 'Stats', icon: BarChart3 },
    { id: 'logs' as Tab, label: 'Live Console', short: 'Logs', icon: Terminal },
  ];

  return (
    <div className="min-h-screen bg-[var(--bg-app)] text-slate-100 flex flex-col font-sans transition-colors duration-300">
      {/* Top Cybernetic Header Navbar - Pixel-aligned single-row layout */}
      <header className="border-b border-[var(--border-subtle)] bg-[var(--bg-header)] backdrop-blur-md sticky top-0 z-50 transition-colors">
        <div className="max-w-7xl mx-auto h-14 px-4 sm:px-6 flex items-center justify-between gap-4">
          
          {/* Brand Logo */}
          <div className="flex items-center gap-3 shrink-0">
            <div className="w-8 h-8 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-center justify-center shadow-[0_0_15px_var(--primary-glow)] transition-all">
              <Cpu className="w-4 h-4 text-[var(--primary)]" />
            </div>

            <div className="flex items-center gap-2">
              <span className="text-base font-bold text-white tracking-tight font-mono">
                WIRE<span className="text-[var(--primary)] wire-text-glow font-extrabold transition-colors">LLM</span>
              </span>
              <span className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)]">
                v0.2.1
              </span>
              <span className="hidden sm:inline-flex items-center gap-1.5 text-[11px] font-mono text-slate-400 ml-1">
                <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] animate-pulse shadow-[0_0_8px_var(--primary)]" />
                &lt;2ms wire latency
              </span>
            </div>
          </div>

          {/* Navigation Tabs */}
          <nav className="flex items-center gap-1 bg-[#010307]/80 p-1 rounded-xl border border-white/[0.08]">
            {navItems.map((item) => {
              const Icon = item.icon;
              const isActive = activeTab === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => setActiveTab(item.id)}
                  className={cn(
                    'h-8 px-3 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-all select-none',
                    isActive
                      ? 'bg-[var(--primary-bg-badge)] text-[var(--primary)] border border-[var(--primary-border)] shadow-[0_0_15px_-2px_var(--primary-glow-subtle)] font-semibold'
                      : 'text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent'
                  )}
                >
                  <Icon className={cn('w-3.5 h-3.5', isActive ? 'text-[var(--primary)]' : 'text-slate-500')} />
                  <span className="hidden md:inline">{item.label}</span>
                  <span className="md:hidden">{item.short}</span>
                </button>
              );
            })}
          </nav>

          {/* Right Status Hub */}
          <div className="flex items-center gap-2 shrink-0">
            {/* Live WebSocket Status Badge */}
            <div className="flex items-center gap-2 bg-[#010307]/90 h-8 px-2.5 rounded-lg border border-white/[0.08] text-xs font-mono">
              <span
                className={cn(
                  'w-2 h-2 rounded-full inline-block',
                  wsConnected
                    ? 'bg-[var(--primary)] shadow-[0_0_10px_var(--primary)] animate-pulse'
                    : isConnected
                    ? 'bg-amber-400 shadow-[0_0_8px_rgba(245,158,11,0.8)]'
                    : 'bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.8)]'
                )}
              />
              <span className="text-slate-300 text-[11px] tracking-tight">
                {wsConnected ? 'WS LIVE' : isConnected ? `PORT :${window.location.port || '8765'}` : 'OFFLINE'}
              </span>
            </div>

            {/* MCP SSE Live Indicator */}
            <div
              title="Native Model Context Protocol Server active at /mcp/sse"
              className="hidden lg:flex items-center gap-1.5 h-8 px-2.5 rounded-lg text-[11px] font-mono text-[var(--primary)] bg-[var(--primary-bg-badge)] border border-[var(--primary-border)]"
            >
              <Radio className="w-3.5 h-3.5 text-[var(--primary)] animate-pulse" />
              <span>MCP SSE</span>
            </div>

            {/* Theme Selector */}
            <ThemeSelector />

            {/* Manual Refresh Button */}
            <Button
              variant="secondary"
              size="icon"
              onClick={handleManualRefresh}
              title="Refresh Telemetry State"
              className="h-8 w-8 hover:border-[var(--primary-border)] hover:text-[var(--primary)]"
            >
              <RefreshCw
                className={cn('w-3.5 h-3.5 text-slate-400', isRefreshing && 'animate-spin text-[var(--primary)]')}
              />
            </Button>
          </div>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 max-w-7xl w-full mx-auto p-4 sm:p-6 space-y-6">
        {activeTab === 'overview' && (
          <Overview
            status={status}
            onRefresh={handleManualRefresh}
            liveEvent={latestLiveEvent}
            recentStreams={recentStreams}
          />
        )}
        {activeTab === 'providers' && <ProvidersStudio status={status} onRefresh={handleManualRefresh} />}
        {activeTab === 'models' && <VirtualModelsStudio status={status} onRefresh={handleManualRefresh} />}
        {activeTab === 'analytics' && <AnalyticsView />}
        {activeTab === 'logs' && <LogsView />}
      </main>

      {/* Infrastructure Footer */}
      <footer className="border-t border-white/[0.06] bg-[#04060a] py-3.5 px-6 text-center text-xs text-slate-500 font-mono flex flex-col sm:flex-row items-center justify-between gap-2 max-w-7xl mx-auto w-full">
        <div className="flex items-center gap-2">
          <span className="w-1.5 h-1.5 rounded-full bg-cyan-400" />
          <span>WireLLM Infrastructure Daemon</span>
          <span className="text-slate-600">&bull;</span>
          <span>SQLite WAL &bull; AES-256 Vault &bull; OTLP Spans</span>
        </div>
        <div className="text-[11px] text-slate-600">
          Pure Rust &bull; ~20 MB RAM &bull; Sub-millisecond Execution
        </div>
      </footer>
    </div>
  );
};
