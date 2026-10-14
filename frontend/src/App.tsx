import React, { useEffect, useRef, useState } from 'react';
import { ApiStatusResponse, LiveStreamEvent, AuthStatusResponse } from './types';
import { fetchStatus, fetchAuthStatus, logout } from './api';
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
  Shield,
  LogOut,
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
  const [authStatus, setAuthStatus] = useState<AuthStatusResponse | null>(null);
  const [authLoading, setAuthLoading] = useState<boolean>(true);
  const wsRef = useRef<WebSocket | null>(null);

  // Check authentication status on startup
  useEffect(() => {
    fetchAuthStatus()
      .then((res) => {
        setAuthStatus(res);
        setAuthLoading(false);
      })
      .catch((err) => {
        console.warn('Failed to fetch auth status', err);
        setAuthLoading(false);
      });
  }, []);

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

  if (authLoading) {
    return (
      <div className="min-h-screen bg-[var(--bg-app)] flex items-center justify-center font-mono text-xs text-slate-400">
        <div className="flex items-center gap-2.5">
          <span className="w-2.5 h-2.5 rounded-full bg-[var(--primary)] animate-ping" />
          <span>INITIALIZING WIRELLM SECURITY MESH...</span>
        </div>
      </div>
    );
  }

  // Google OAuth Gateway Guard
  if (authStatus?.google_auth_enabled && !authStatus?.authenticated) {
    return (
      <div className="min-h-screen bg-[var(--bg-app)] text-slate-100 flex flex-col items-center justify-center p-4 font-sans select-none relative overflow-hidden">
        {/* Ambient radial glow */}
        <div className="absolute top-1/4 left-1/2 -translate-x-1/2 w-[500px] h-[500px] bg-[var(--primary)]/10 rounded-full blur-[130px] pointer-events-none" />

        <div className="w-full max-w-md bg-[#050814] border-2 border-[var(--primary-border)] rounded-2xl p-8 shadow-[0_25px_60px_rgba(0,0,0,0.95),0_0_35px_var(--primary-glow-subtle)] relative z-10 flex flex-col items-center text-center space-y-6">
          {/* Brand Logo */}
          <div className="w-16 h-16 rounded-2xl bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] flex items-center justify-center shadow-[0_0_25px_var(--primary-glow)]">
            <Cpu className="w-8 h-8 text-[var(--primary)] animate-pulse" />
          </div>

          {/* Title & Badge */}
          <div className="space-y-1.5">
            <div className="flex items-center justify-center gap-2">
              <span className="text-2xl font-black text-white font-mono tracking-tight">
                WIRE<span className="text-[var(--primary)] wire-text-glow">LLM</span>
              </span>
              <Badge variant="solid-primary" size="xs">CLUSTER</Badge>
            </div>
            <p className="text-xs text-slate-400 font-mono">
              Autonomous Circuit Router &amp; Multi-Provider Mesh
            </p>
          </div>

          {/* Protected cluster banner */}
          <div className="w-full p-4 rounded-xl bg-white/[0.03] border border-white/10 space-y-2 text-left text-xs font-mono">
            <div className="flex items-center gap-2 text-white font-semibold">
              <Shield className="w-4 h-4 text-[var(--secondary)]" />
              <span>Protected Cluster Instance</span>
            </div>
            <p className="text-[11px] text-slate-400 leading-relaxed font-sans">
              This deployment is secured with Google OAuth. Please sign in with an authorized Google account to access routing topologies, provider meters, and telemetry.
            </p>
          </div>

          {/* Google Sign-in Button */}
          <a
            href="/auth/google/login"
            className="w-full h-11 px-4 rounded-xl bg-white text-slate-900 hover:bg-slate-100 font-medium text-sm flex items-center justify-center gap-3 transition-all shadow-[0_0_20px_rgba(255,255,255,0.2)] hover:shadow-[0_0_25px_rgba(255,255,255,0.4)] hover:scale-[1.01] active:scale-[0.99] font-sans"
          >
            <svg className="w-4 h-4 shrink-0" viewBox="0 0 24 24">
              <path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
              <path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
              <path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.06H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.94l2.85-2.22.81-.63z"/>
              <path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.06l3.66 2.84c.87-2.6 3.3-4.52 6.16-4.52z"/>
            </svg>
            <span className="font-semibold">Sign in with Google</span>
          </a>

          {/* Theme Palette Bar */}
          <div className="pt-2 border-t border-white/[0.06] w-full flex items-center justify-between text-[11px] text-slate-500 font-mono">
            <span>Color Theme:</span>
            <ThemeSelector />
          </div>
        </div>
      </div>
    );
  }

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
            {/* User Profile (Google Auth) */}
            {authStatus?.user && (
              <div className="flex items-center gap-2 bg-[#010307]/90 h-8 px-2 rounded-lg border border-white/10 text-xs font-mono">
                {authStatus.user.picture ? (
                  <img
                    src={authStatus.user.picture}
                    alt={authStatus.user.name}
                    className="w-5 h-5 rounded-full border border-[var(--primary)] shrink-0"
                  />
                ) : (
                  <div className="w-5 h-5 rounded-full bg-[var(--primary)] text-black font-extrabold flex items-center justify-center text-[10px] shrink-0">
                    {(authStatus.user.name || authStatus.user.email).charAt(0).toUpperCase()}
                  </div>
                )}
                <span className="hidden xl:inline text-slate-300 text-[11px] max-w-[120px] truncate" title={authStatus.user.email}>
                  {authStatus.user.name || authStatus.user.email}
                </span>
                <button
                  onClick={() => logout()}
                  title={`Sign out (${authStatus.user.email})`}
                  className="p-1 rounded text-slate-400 hover:text-rose-400 hover:bg-white/[0.06] transition-colors"
                >
                  <LogOut className="w-3.5 h-3.5" />
                </button>
              </div>
            )}

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
          <span className="w-1.5 h-1.5 rounded-full bg-[var(--primary)] shadow-[0_0_6px_var(--primary)]" />
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
