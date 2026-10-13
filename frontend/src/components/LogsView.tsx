import React, { useEffect, useState, useRef } from 'react';
import { LogEntry } from '../types';
import { fetchLogs } from '../api';
import { Terminal, Search, RotateCcw, Play, Pause } from 'lucide-react';
import { Card, Button, Input, Select, Badge } from '../ui';

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

  const getBadgeVariant = (level: string) => {
    switch (level.toUpperCase()) {
      case 'ERROR':
        return 'rose';
      case 'WARN':
        return 'amber';
      case 'INFO':
        return 'cyan';
      default:
        return 'neutral';
    }
  };

  return (
    <div className="space-y-4">
      {/* Controls Bar */}
      <Card className="p-4 flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-cyan-950/40 border border-cyan-800/40 text-cyan-400 shrink-0">
            <Terminal className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-sm font-bold text-white font-mono flex items-center gap-2">
              Wire Daemon Ring-Buffer Console
              <Badge variant="cyan">IN-MEMORY</Badge>
            </h2>
            <p className="text-[11px] text-slate-400">Zero disk I/O lock contention during burst traffic</p>
          </div>
        </div>

        <div className="flex items-center gap-2 flex-wrap">
          {/* Search Box */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
            <Input
              type="text"
              placeholder="Search message, req_id..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="pl-8 w-44"
              mono
            />
          </div>

          {/* Level Filter */}
          <Select
            value={filterLevel}
            onChange={(e) => setFilterLevel(e.target.value)}
            mono
          >
            <option value="ALL">All Levels</option>
            <option value="INFO">INFO</option>
            <option value="WARN">WARN</option>
            <option value="ERROR">ERROR</option>
            <option value="DEBUG">DEBUG</option>
          </Select>

          {/* Live / Pause Button */}
          <Button
            variant={isLive ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => setIsLive(!isLive)}
          >
            {isLive ? <Pause className="w-3 h-3" /> : <Play className="w-3 h-3" />}
            {isLive ? 'Live' : 'Paused'}
          </Button>

          {/* Refresh Button */}
          <Button
            variant="secondary"
            size="icon"
            onClick={loadLogs}
            title="Refresh logs now"
          >
            <RotateCcw className="w-4 h-4" />
          </Button>
        </div>
      </Card>

      {/* Terminal View Container */}
      <Card className="bg-[#030508] p-4 font-mono text-xs overflow-x-auto h-[600px] overflow-y-auto">
        <div className="space-y-1.5">
          {filteredLogs.map((entry, idx) => (
            <div
              key={idx}
              className="flex items-start gap-2.5 hover:bg-white/[0.03] py-1 px-1.5 rounded transition-colors"
            >
              <span className="text-slate-600 text-[11px] whitespace-nowrap">
                {entry.timestamp.slice(11, 19)}
              </span>
              <Badge variant={getBadgeVariant(entry.level)} size="xs">
                {entry.level}
              </Badge>
              {entry.provider && (
                <span className="text-cyan-400 text-[11px] font-semibold">[{entry.provider}]</span>
              )}
              {entry.request_id && (
                <span className="text-sky-400 text-[11px]">({entry.request_id})</span>
              )}
              <span className="text-slate-300 break-all">{entry.message}</span>
            </div>
          ))}

          {filteredLogs.length === 0 && (
            <div className="text-center py-16 text-slate-600 font-sans text-xs">
              No log records match current filter.
            </div>
          )}

          <div ref={logsEndRef} />
        </div>
      </Card>
    </div>
  );
};
