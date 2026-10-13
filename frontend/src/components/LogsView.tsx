import React, { useEffect, useState, useRef } from 'react';
import { LogEntry } from '../types';
import { fetchLogs } from '../api';
import { Terminal, Search, Filter, RotateCcw, Play, Pause, ArrowDown } from 'lucide-react';

export const LogsView: React.FC = () => {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [filterLevel, setFilterLevel] = useState<string>('ALL');
  const [searchTerm, setSearchTerm] = useState<string>('');
  const [isAutoScroll, setIsAutoScroll] = useState<boolean>(true);
  const [isLive, setIsLive] = useState<boolean>(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  const loadLogs = async () => {
    try {
      const res = await fetchLogs(200);
      setLogs(res.entries || []);
    } catch (err) {
      console.error('Failed to fetch logs', err);
    }
  };

  useEffect(() => {
    loadLogs();
    if (!isLive) return;
    const interval = setInterval(loadLogs, 2000);
    return () => clearInterval(interval);
  }, [isLive]);

  useEffect(() => {
    if (isAutoScroll && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs, isAutoScroll]);

  const filteredLogs = logs.filter((entry) => {
    if (filterLevel !== 'ALL' && entry.level.toUpperCase() !== filterLevel) {
      return false;
    }
    if (searchTerm) {
      const term = searchTerm.toLowerCase();
      return (
        entry.message.toLowerCase().includes(term) ||
        (entry.request_id && entry.request_id.toLowerCase().includes(term)) ||
        (entry.provider && entry.provider.toLowerCase().includes(term))
      );
    }
    return true;
  });

  const getLevelBadge = (level: string) => {
    switch (level.toUpperCase()) {
      case 'ERROR':
        return 'text-rose-400 bg-rose-950/60 border-rose-800/80';
      case 'WARN':
        return 'text-amber-400 bg-amber-950/60 border-amber-800/80';
      case 'INFO':
        return 'text-sky-400 bg-sky-950/60 border-sky-800/80';
      case 'DEBUG':
        return 'text-slate-400 bg-slate-800/60 border-slate-700/80';
      default:
        return 'text-slate-300 bg-slate-800 border-slate-700';
    }
  };

  return (
    <div className="space-y-4">
      {/* Controls Bar */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 bg-slate-900 border border-slate-800 rounded-xl p-4 shadow-lg">
        <div className="flex items-center gap-3">
          <Terminal className="w-5 h-5 text-brand-500" />
          <div>
            <h2 className="text-sm font-semibold text-white">Live Ring-Buffer Log Stream</h2>
            <p className="text-[11px] text-slate-400">Captured in memory without disk I/O bottlenecks</p>
          </div>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          {/* Search Box */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-slate-400 absolute left-2.5 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              placeholder="Search message, req_id..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="bg-slate-950 border border-slate-800 rounded-lg pl-8 pr-3 py-1.5 text-xs text-white placeholder-slate-500 focus:outline-none focus:border-brand-500 w-44"
            />
          </div>

          {/* Level Filter */}
          <select
            value={filterLevel}
            onChange={(e) => setFilterLevel(e.target.value)}
            className="bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-300 focus:outline-none focus:border-brand-500"
          >
            <option value="ALL">All Levels</option>
            <option value="INFO">INFO</option>
            <option value="WARN">WARN</option>
            <option value="ERROR">ERROR</option>
            <option value="DEBUG">DEBUG</option>
          </select>

          {/* Live / Pause Button */}
          <button
            onClick={() => setIsLive(!isLive)}
            className={`px-2.5 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5 border transition-colors ${
              isLive
                ? 'bg-emerald-950/40 text-emerald-400 border-emerald-800/80 hover:bg-emerald-950/60'
                : 'bg-slate-800 text-slate-400 border-slate-700 hover:text-white'
            }`}
          >
            {isLive ? <Pause className="w-3 h-3" /> : <Play className="w-3 h-3" />}
            {isLive ? 'Live' : 'Paused'}
          </button>

          {/* Refresh Button */}
          <button
            onClick={loadLogs}
            className="p-1.5 bg-slate-800 hover:bg-slate-700 text-slate-300 rounded-lg transition-colors"
            title="Refresh logs now"
          >
            <RotateCcw className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Terminal View Container */}
      <div className="bg-slate-950 border border-slate-800 rounded-xl p-4 shadow-2xl font-mono text-xs overflow-x-auto h-[600px] overflow-y-auto">
        <div className="space-y-1.5">
          {filteredLogs.map((entry, idx) => (
            <div
              key={idx}
              className="flex items-start gap-2.5 hover:bg-slate-900/60 py-1 px-1.5 rounded transition-colors"
            >
              <span className="text-slate-500 text-[11px] whitespace-nowrap">
                {entry.timestamp.slice(11, 19)}
              </span>
              <span
                className={`px-1.5 py-0.2 rounded border text-[10px] font-bold tracking-wider ${getLevelBadge(
                  entry.level
                )}`}
              >
                {entry.level}
              </span>
              {entry.provider && (
                <span className="text-brand-400 text-[11px] font-semibold">[{entry.provider}]</span>
              )}
              {entry.request_id && (
                <span className="text-sky-400 text-[11px]">({entry.request_id})</span>
              )}
              <span className="text-slate-300 break-all">{entry.message}</span>
            </div>
          ))}

          {filteredLogs.length === 0 && (
            <div className="text-center py-12 text-slate-600 font-sans text-xs">
              No log records match current filter.
            </div>
          )}

          <div ref={logsEndRef} />
        </div>
      </div>
    </div>
  );
};
