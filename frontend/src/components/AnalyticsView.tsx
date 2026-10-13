import React, { useEffect, useState } from 'react';
import { AnalyticsResponse, DailyStats } from '../types';
import { fetchAnalytics } from '../api';
import {
  BarChart3,
  TrendingUp,
  Zap,
  Clock,
  Calendar,
  Activity,
  Layers,
} from 'lucide-react';
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  Legend,
  LineChart,
  Line,
} from 'recharts';

export const AnalyticsView: React.FC = () => {
  const [data, setData] = useState<AnalyticsResponse | null>(null);
  const [days, setDays] = useState(30);
  const [loading, setLoading] = useState(true);

  const loadData = async (d: number) => {
    setLoading(true);
    try {
      const res = await fetchAnalytics(d);
      setData(res);
    } catch (err) {
      console.error(err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadData(days);
  }, [days]);

  // Aggregate by date for charts
  const chartDataMap: Record<string, { date: string; input_tokens: number; output_tokens: number; requests: number; latency_sum: number; latency_count: number }> = {};

  if (data?.stats) {
    for (const s of data.stats) {
      if (!chartDataMap[s.date]) {
        chartDataMap[s.date] = {
          date: s.date,
          input_tokens: 0,
          output_tokens: 0,
          requests: 0,
          latency_sum: 0,
          latency_count: 0,
        };
      }
      chartDataMap[s.date].input_tokens += s.total_input_tokens;
      chartDataMap[s.date].output_tokens += s.total_output_tokens;
      chartDataMap[s.date].requests += s.total_requests;
      if (s.avg_latency_ms > 0) {
        chartDataMap[s.date].latency_sum += s.avg_latency_ms * s.total_requests;
        chartDataMap[s.date].latency_count += s.total_requests;
      }
    }
  }

  const chartData = Object.values(chartDataMap).map((d) => ({
    date: d.date.slice(5), // MM-DD
    'Input Tokens': d.input_tokens,
    'Output Tokens': d.output_tokens,
    'Total Tokens': d.input_tokens + d.output_tokens,
    Requests: d.requests,
    'Avg Latency (ms)': d.latency_count > 0 ? Math.round(d.latency_sum / d.latency_count) : 0,
  }));

  const formatTokens = (tokens: number) => {
    if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
    if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}k`;
    return tokens.toLocaleString();
  };

  return (
    <div className="space-y-6">
      {/* Top Banner & Range Selector */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg">
        <div>
          <h2 className="text-lg font-semibold text-white flex items-center gap-2">
            <BarChart3 className="w-5 h-5 text-brand-500" />
            Token Consumption &amp; Usage Analytics
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Real-time daily aggregation stored persistently in SQLite.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <span className="text-xs text-slate-400 font-medium">History Range:</span>
          {[7, 30, 90].map((d) => (
            <button
              key={d}
              onClick={() => setDays(d)}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors ${
                days === d
                  ? 'bg-brand-600 text-white shadow'
                  : 'bg-slate-800 hover:bg-slate-700 text-slate-300'
              }`}
            >
              {d} Days
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <div className="flex items-center justify-center p-12 text-slate-400">
          <Activity className="w-6 h-6 animate-spin mr-2 text-brand-500" />
          <span>Loading analytics data...</span>
        </div>
      ) : (
        <>
          {/* Key Analytics Cards */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg">
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Total Tokens
                </span>
                <Zap className="w-5 h-5 text-amber-400" />
              </div>
              <div className="mt-3 text-2xl font-bold text-white tracking-tight">
                {formatTokens((data?.total_input_tokens || 0) + (data?.total_output_tokens || 0))}
              </div>
              <div className="mt-1 text-xs text-slate-500">
                {formatTokens(data?.total_input_tokens || 0)} prompt &bull;{' '}
                {formatTokens(data?.total_output_tokens || 0)} completion
              </div>
            </div>

            <div className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg">
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Total Requests
                </span>
                <Activity className="w-5 h-5 text-sky-400" />
              </div>
              <div className="mt-3 text-2xl font-bold text-white tracking-tight">
                {(data?.total_requests || 0).toLocaleString()}
              </div>
              <div className="mt-1 text-xs text-slate-500">
                Routed across active providers
              </div>
            </div>

            <div className="bg-slate-900 border border-slate-800 rounded-xl p-5 shadow-lg">
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Daily Active Models
                </span>
                <Layers className="w-5 h-5 text-purple-400" />
              </div>
              <div className="mt-3 text-2xl font-bold text-white tracking-tight">
                {new Set(data?.stats.map((s) => s.model_name)).size}
              </div>
              <div className="mt-1 text-xs text-slate-500">
                Across {new Set(data?.stats.map((s) => s.provider_name)).size} upstream providers
              </div>
            </div>
          </div>

          {/* Daily Token Chart */}
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-xl">
            <h3 className="text-sm font-semibold text-white mb-4 flex items-center gap-2">
              <TrendingUp className="w-4 h-4 text-brand-500" />
              Daily Token Volume Breakdown
            </h3>
            <div className="h-72 w-full">
              {chartData.length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={chartData}>
                    <XAxis dataKey="date" stroke="#64748b" fontSize={11} />
                    <YAxis stroke="#64748b" fontSize={11} tickFormatter={(v) => formatTokens(v)} />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: '#0f172a',
                        border: '1px solid #334155',
                        borderRadius: '0.5rem',
                        fontSize: '12px',
                      }}
                    />
                    <Legend wrapperStyle={{ fontSize: '12px' }} />
                    <Bar dataKey="Input Tokens" stackId="a" fill="#0284c7" />
                    <Bar dataKey="Output Tokens" stackId="a" fill="#10b981" />
                  </BarChart>
                </ResponsiveContainer>
              ) : (
                <div className="flex items-center justify-center h-full text-slate-500 text-xs">
                  No historical traffic logged yet. Run requests through http://localhost:{window.location.port || '8088'}/v1/chat/completions!
                </div>
              )}
            </div>
          </div>

          {/* Daily Latency Trend */}
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-xl">
            <h3 className="text-sm font-semibold text-white mb-4 flex items-center gap-2">
              <Clock className="w-4 h-4 text-sky-400" />
              Average Upstream Response Latency (ms)
            </h3>
            <div className="h-56 w-full">
              {chartData.length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={chartData}>
                    <XAxis dataKey="date" stroke="#64748b" fontSize={11} />
                    <YAxis stroke="#64748b" fontSize={11} unit="ms" />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: '#0f172a',
                        border: '1px solid #334155',
                        borderRadius: '0.5rem',
                        fontSize: '12px',
                      }}
                    />
                    <Line
                      type="monotone"
                      dataKey="Avg Latency (ms)"
                      stroke="#38bdf8"
                      strokeWidth={2}
                      dot={{ r: 3 }}
                    />
                  </LineChart>
                </ResponsiveContainer>
              ) : (
                <div className="flex items-center justify-center h-full text-slate-500 text-xs">
                  No latency stats logged yet.
                </div>
              )}
            </div>
          </div>

          {/* Model Breakdown Table */}
          <div className="bg-slate-900 border border-slate-800 rounded-xl p-6 shadow-xl">
            <h3 className="text-sm font-semibold text-white mb-4">
              Model Usage Breakdown (Last {days} Days)
            </h3>
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs text-slate-300">
                <thead className="bg-slate-950/60 text-slate-400 uppercase tracking-wider border-b border-slate-800">
                  <tr>
                    <th className="py-2.5 px-3 font-medium">Date</th>
                    <th className="py-2.5 px-3 font-medium">Provider</th>
                    <th className="py-2.5 px-3 font-medium">Model</th>
                    <th className="py-2.5 px-3 font-medium">Requests</th>
                    <th className="py-2.5 px-3 font-medium">Prompt Tokens</th>
                    <th className="py-2.5 px-3 font-medium">Completion Tokens</th>
                    <th className="py-2.5 px-3 font-medium">Avg Latency</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/60 font-mono">
                  {data?.stats.map((s, idx) => (
                    <tr key={`${s.date}-${s.provider_name}-${s.model_name}-${idx}`} className="hover:bg-slate-800/40">
                      <td className="py-2.5 px-3 text-slate-400 font-sans">{s.date}</td>
                      <td className="py-2.5 px-3 font-semibold text-slate-200 capitalize font-sans">
                        {s.provider_name}
                      </td>
                      <td className="py-2.5 px-3 text-slate-300">{s.model_name}</td>
                      <td className="py-2.5 px-3 text-slate-200">{s.total_requests}</td>
                      <td className="py-2.5 px-3 text-sky-400">{s.total_input_tokens.toLocaleString()}</td>
                      <td className="py-2.5 px-3 text-emerald-400">{s.total_output_tokens.toLocaleString()}</td>
                      <td className="py-2.5 px-3 text-slate-300">
                        {s.avg_latency_ms > 0 ? `${s.avg_latency_ms.toFixed(0)} ms` : '-'}
                      </td>
                    </tr>
                  ))}
                  {(!data?.stats || data.stats.length === 0) && (
                    <tr>
                      <td colSpan={7} className="py-6 text-center text-slate-500 font-sans">
                        No transactions recorded in the selected period.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </>
      )}
    </div>
  );
};
