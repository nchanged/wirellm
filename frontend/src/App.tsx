import React, { useEffect, useState } from 'react';
import { ApiStatusResponse } from './types';
import { fetchStatus } from './api';
import { Overview } from './components/Overview';
import { ProvidersStudio } from './components/ProvidersStudio';
import { VirtualModelsStudio } from './components/VirtualModelsStudio';
import { AnalyticsView } from './components/AnalyticsView';
import { LogsView } from './components/LogsView';
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

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 4000);
    return () => clearInterval(interval);
  }, []);

  const navItems = [
    { id: 'overview' as Tab, label: 'Routing Matrix', short: 'Router', icon: Workflow },
    { id: 'providers' as Tab, label: 'Providers & Vault', short: 'Providers', icon: Server },
    { id: 'models' as Tab, label: 'Virtual Models', short: 'Models', icon: SlidersHorizontal },
    { id: 'analytics' as Tab, label: 'Token Analytics', short: 'Stats', icon: BarChart3 },
    { id: 'logs' as Tab, label: 'Live Console', short: 'Logs', icon: Terminal },
  ];

  return (
    <div className="min-h-screen bg-[#030508] text-slate-100 flex flex-col font-sans selection:bg-cyan-500/20 selection:text-cyan-300">
      {/* Top Cybernetic Header Navbar - Pixel-aligned single-row layout */}
      <header className="border-b border-white/[0.08] bg-[#05080f]/95 backdrop-blur-md sticky top-0 z-50">
        <div className="max-w-7xl mx-auto h-14 px-4 sm:px-6 flex items-center justify-between gap-4">
          
          {/* Brand Logo */}
          <div className="flex items-center gap-3 shrink-0">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-500/20 to-indigo-500/10 border border-cyan-500/30 flex items-center justify-center shadow-[0_0_12px_rgba(6,182,212,0.25)]">
              <Cpu className="w-4 h-4 text-cyan-400" />
            </div>

            <div className="flex items-center gap-2">
              <span className="text-base font-bold text-white tracking-tight font-mono">
                WIRE<span className="text-cyan-400 font-extrabold">LLM</span>
              </span>
              <Badge variant="cyan" size="xs">
                v0.2.1
              </Badge>
              <span className="hidden sm:inline-flex items-center gap-1.5 text-[11px] font-mono text-slate-500 ml-1">
                <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-pulse" />
                &lt;2ms latency
              </span>
            </div>
          </div>

          {/* Navigation Tabs */}
          <nav className="flex items-center gap-1 bg-[#030508] p-1 rounded-xl border border-white/[0.08]">
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
                      ? 'bg-cyan-500/15 text-cyan-300 border border-cyan-500/40 shadow-[0_0_12px_-2px_rgba(6,182,212,0.3)]'
                      : 'text-slate-400 hover:text-slate-200 hover:bg-white/[0.04] border border-transparent'
                  )}
                >
                  <Icon className={cn('w-3.5 h-3.5', isActive ? 'text-cyan-400' : 'text-slate-500')} />
                  <span className="hidden md:inline">{item.label}</span>
                  <span className="md:hidden">{item.short}</span>
                </button>
              );
            })}
          </nav>

          {/* Right Status Hub */}
          <div className="flex items-center gap-2 shrink-0">
            {/* Live Gateway Port Badge */}
            <div className="hidden sm:flex items-center gap-2 bg-[#030508] h-8 px-2.5 rounded-lg border border-white/[0.08] text-xs font-mono">
              <span
                className={cn(
                  'w-2 h-2 rounded-full inline-block',
                  isConnected
                    ? 'bg-cyan-400 shadow-[0_0_8px_rgba(6,182,212,0.8)] animate-pulse'
                    : 'bg-rose-500 shadow-[0_0_8px_rgba(244,63,94,0.8)]'
                )}
              />
              <span className="text-slate-300 text-[11px] tracking-tight">
                {isConnected ? `PORT :${window.location.port || '8765'}` : 'OFFLINE'}
              </span>
            </div>

            {/* MCP SSE Live Indicator */}
            <div
              title="Native Model Context Protocol Server active at /mcp/sse"
              className="hidden lg:flex items-center gap-1.5 h-8 px-2.5 rounded-lg text-[11px] font-mono text-indigo-300 bg-indigo-950/40 border border-indigo-800/50"
            >
              <Radio className="w-3.5 h-3.5 text-indigo-400 animate-pulse" />
              <span>MCP SSE</span>
            </div>

            {/* Manual Refresh Button */}
            <Button
              variant="secondary"
              size="icon"
              onClick={loadStatus}
              title="Refresh Telemetry State"
              className="h-8 w-8"
            >
              <RefreshCw
                className={cn('w-3.5 h-3.5 text-slate-400', isRefreshing && 'animate-spin text-cyan-400')}
              />
            </Button>
          </div>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 max-w-7xl w-full mx-auto p-4 sm:p-6 space-y-6">
        {activeTab === 'overview' && <Overview status={status} onRefresh={loadStatus} />}
        {activeTab === 'providers' && <ProvidersStudio status={status} onRefresh={loadStatus} />}
        {activeTab === 'models' && <VirtualModelsStudio status={status} onRefresh={loadStatus} />}
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
