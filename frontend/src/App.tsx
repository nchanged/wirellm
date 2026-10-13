import React, { useEffect, useState } from 'react';
import { ApiStatusResponse } from './types';
import { fetchStatus } from './api';
import { Overview } from './components/Overview';
import { ProvidersStudio } from './components/ProvidersStudio';
import { VirtualModelsStudio } from './components/VirtualModelsStudio';
import { AnalyticsView } from './components/AnalyticsView';
import { LogsView } from './components/LogsView';
import {
  Zap,
  Server,
  Layers,
  BarChart3,
  Terminal,
  Activity,
  Shield,
  Bot,
  RefreshCw,
} from 'lucide-react';

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

  return (
    <div className="min-h-screen bg-slate-950 text-slate-100 flex flex-col font-sans">
      {/* Top Header Navbar */}
      <header className="border-b border-slate-800 bg-slate-900/80 backdrop-blur sticky top-0 z-40 px-6 py-3.5">
        <div className="max-w-7xl mx-auto flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-gradient-to-tr from-brand-600 to-emerald-400 flex items-center justify-center shadow-lg shadow-brand-500/20">
              <Zap className="w-4 h-4 text-slate-950 fill-current" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <span className="text-base font-bold text-white tracking-tight">WireLLM</span>
                <span className="px-1.5 py-0.5 rounded text-[10px] font-bold bg-brand-500/20 text-brand-400 border border-brand-500/30">
                  v2.0
                </span>
                <span className="text-xs text-slate-500">Autonomous LLM Gateway</span>
              </div>
            </div>
          </div>

          {/* Navigation Tabs */}
          <nav className="hidden md:flex items-center gap-1 bg-slate-950/80 p-1 rounded-xl border border-slate-800">
            <button
              onClick={() => setActiveTab('overview')}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
                activeTab === 'overview'
                  ? 'bg-slate-800 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Activity className="w-3.5 h-3.5 text-brand-400" />
              Dynamic Router
            </button>

            <button
              onClick={() => setActiveTab('providers')}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
                activeTab === 'providers'
                  ? 'bg-slate-800 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Server className="w-3.5 h-3.5 text-sky-400" />
              Providers Studio
            </button>

            <button
              onClick={() => setActiveTab('models')}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
                activeTab === 'models'
                  ? 'bg-slate-800 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Layers className="w-3.5 h-3.5 text-purple-400" />
              Virtual Models
            </button>

            <button
              onClick={() => setActiveTab('analytics')}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
                activeTab === 'analytics'
                  ? 'bg-slate-800 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <BarChart3 className="w-3.5 h-3.5 text-amber-400" />
              Token Analytics
            </button>

            <button
              onClick={() => setActiveTab('logs')}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
                activeTab === 'logs'
                  ? 'bg-slate-800 text-white shadow'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              <Terminal className="w-3.5 h-3.5 text-emerald-400" />
              Live Logs
            </button>
          </nav>

          {/* Right Status / MCP Pill */}
          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5 bg-slate-950 px-2.5 py-1 rounded-lg border border-slate-800 text-xs">
              <span
                className={`w-2 h-2 rounded-full ${
                  isConnected ? 'bg-emerald-400 animate-pulse' : 'bg-rose-500'
                }`}
              />
              <span className="text-slate-300 font-mono text-[11px]">
                {isConnected ? `ONLINE :${window.location.port || '8088'}` : 'OFFLINE'}
              </span>
            </div>

            <div
              title="MCP Server active on /mcp/sse and /mcp/messages"
              className="hidden lg:flex items-center gap-1 text-[11px] font-mono text-purple-300 bg-purple-950/40 border border-purple-800/60 px-2 py-0.5 rounded cursor-help"
            >
              <Bot className="w-3.5 h-3.5" />
              MCP Active
            </div>

            <button
              onClick={loadStatus}
              title="Reload Status"
              className="p-1.5 text-slate-400 hover:text-white rounded-lg hover:bg-slate-800 transition-colors"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin text-brand-400' : ''}`} />
            </button>
          </div>
        </div>
      </header>

      {/* Main Content Area */}
      <main className="flex-1 max-w-7xl w-full mx-auto p-6">
        {activeTab === 'overview' && <Overview status={status} onRefresh={loadStatus} />}
        {activeTab === 'providers' && <ProvidersStudio status={status} onRefresh={loadStatus} />}
        {activeTab === 'models' && <VirtualModelsStudio status={status} onRefresh={loadStatus} />}
        {activeTab === 'analytics' && <AnalyticsView />}
        {activeTab === 'logs' && <LogsView />}
      </main>

      {/* Footer */}
      <footer className="border-t border-slate-900 bg-slate-950 py-4 px-6 text-center text-xs text-slate-600 font-mono">
        WireLLM Autonomous Router &bull; SQLite WAL Mode &bull; OpenTelemetry OTLP &bull; MCP SSE Active
      </footer>
    </div>
  );
};
