import React, { useEffect, useState } from 'react';
import { AnalyticsResponse } from '../types';
import { fetchAnalytics } from '../api';
import {
  BarChart3,
  TrendingUp,
  Zap,
  Clock,
  Activity,
  Layers,
  Database,
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
  CartesianGrid,
} from 'recharts';
import {
  Card,
  MetricCard,
  Badge,
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from '../ui';
import { useTheme } from '../theme';

export const AnalyticsView: React.FC = () => {
  const { currentConfig } = useTheme();
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
  const chartDataMap: Record<
    string,
    {
      date: string;
      input_tokens: number;
      output_tokens: number;
      requests: number;
      latency_sum: number;
      latency_count: number;
    }
  > = {};

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
      <Card className="p-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-start gap-3.5">
          <div className="p-2.5 rounded-lg bg-[var(--primary-bg-badge)] border border-[var(--primary-border)] text-[var(--primary)] shrink-0">
            <BarChart3 className="w-5 h-5" />
          </div>
          <div>
            <h2 className="text-base font-bold text-white tracking-wide flex items-center gap-2">
              Wire Telemetry &amp; Token Analytics
              <Badge variant="primary">SQLITE METRICS</Badge>
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              High-resolution transaction accounting. Zero external analytics or telemetry pinging outside your machine.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1.5 bg-[#030508] p-1 rounded-lg border border-white/[0.06]">
          {[7, 30, 90].map((d) => (
            <button
              key={d}
              onClick={() => setDays(d)}
              className={`px-3 py-1.5 rounded-md text-xs font-mono font-semibold transition-all ${
                days === d
                  ? 'bg-[var(--primary)] text-black font-extrabold shadow-[0_0_12px_var(--primary-glow-subtle)]'
                  : 'text-slate-400 hover:text-white hover:bg-white/[0.04]'
              }`}
            >
              {d}D
            </button>
          ))}
        </div>
      </Card>

      {loading ? (
        <Card className="p-20 flex flex-col items-center justify-center text-slate-500 gap-3">
          <Activity className="w-6 h-6 animate-spin text-[var(--primary)]" />
          <span className="text-xs font-mono">Aggregating telemetry logs...</span>
        </Card>
      ) : (
        <>
          {/* Key Analytics Cards using MetricCard */}
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <MetricCard
              label="Total Throughput"
              value={formatTokens((data?.total_input_tokens || 0) + (data?.total_output_tokens || 0))}
              icon={<Zap className="w-4 h-4" />}
              accent="primary"
              subtext={
                <span className="flex items-center gap-1.5">
                  <span className="text-[var(--primary)]">{formatTokens(data?.total_input_tokens || 0)} prompt</span>
                  <span>/</span>
                  <span className="text-indigo-400">{formatTokens(data?.total_output_tokens || 0)} completion</span>
                </span>
              }
            />

            <MetricCard
              label="Requests Dispatched"
              value={(data?.total_requests || 0).toLocaleString()}
              icon={<Activity className="w-4 h-4" />}
              accent="primary"
              subtext="Routed across active upstreams"
            />

            <MetricCard
              label="Active Provider Breadth"
              value={
                <span>
                  {new Set(data?.stats.map((s) => s.model_name)).size}{' '}
                  <span className="text-sm text-slate-500 font-sans font-normal">models</span>
                </span>
              }
              icon={<Layers className="w-4 h-4" />}
              accent="purple"
              subtext={`Across ${new Set(data?.stats.map((s) => s.provider_name)).size} distinct gateways`}
            />
          </div>

          {/* Daily Token Volume Stacked Bar Chart */}
          <Card className="p-5">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-300 flex items-center gap-2">
                <TrendingUp className="w-4 h-4 text-[var(--primary)]" />
                Daily Token Volume (Input vs Output)
              </h3>
              <span className="text-[11px] font-mono text-slate-500">
                Stacked breakdown
              </span>
            </div>
            <div className="h-72 w-full">
              {chartData.length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={chartData} margin={{ top: 10, right: 10, left: -20, bottom: 0 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#ffffff0a" vertical={false} />
                    <XAxis dataKey="date" stroke="#64748b" fontSize={10} fontFamily="JetBrains Mono" />
                    <YAxis
                      stroke="#64748b"
                      fontSize={10}
                      fontFamily="JetBrains Mono"
                      tickFormatter={(v) => formatTokens(v)}
                    />
                    <Tooltip
                      cursor={{ fill: 'rgba(255, 255, 255, 0.04)' }}
                      contentStyle={{
                        backgroundColor: '#05070f',
                        border: '1px solid var(--primary-border)',
                        borderRadius: '0.75rem',
                        fontSize: '11px',
                        fontFamily: 'JetBrains Mono',
                        boxShadow: '0 15px 35px rgba(0,0,0,0.9), 0 0 20px var(--primary-glow-subtle)',
                        color: '#ffffff',
                      }}
                      itemStyle={{ color: '#ffffff' }}
                      labelStyle={{ color: currentConfig.primary, fontWeight: 'bold' }}
                    />
                    <Legend wrapperStyle={{ fontSize: '11px', fontFamily: 'JetBrains Mono', paddingTop: '8px', color: '#94a3b8' }} />
                    <Bar dataKey="Input Tokens" stackId="a" fill={currentConfig.primary} radius={[0, 0, 0, 0]} />
                    <Bar dataKey="Output Tokens" stackId="a" fill="#818cf8" radius={[3, 3, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              ) : (
                <div className="flex items-center justify-center h-full text-slate-600 text-xs font-mono">
                  No historical traffic logged yet. WireLLM will stream analytics as agents send requests.
                </div>
              )}
            </div>
          </Card>

          {/* Daily Latency Line Chart */}
          <Card className="p-5">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-300 flex items-center gap-2">
                <Clock className="w-4 h-4 text-[var(--primary)]" />
                Upstream Provider Round-Trip Latency
              </h3>
              <span className="text-[11px] font-mono text-[var(--primary)]/90">
                Wire overhead &lt;2ms
              </span>
            </div>
            <div className="h-56 w-full">
              {chartData.length > 0 ? (
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={chartData} margin={{ top: 10, right: 10, left: -20, bottom: 0 }}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#ffffff0a" vertical={false} />
                    <XAxis dataKey="date" stroke="#64748b" fontSize={10} fontFamily="JetBrains Mono" />
                    <YAxis stroke="#64748b" fontSize={10} fontFamily="JetBrains Mono" unit="ms" />
                    <Tooltip
                      cursor={{ stroke: 'var(--primary-border)', strokeWidth: 1 }}
                      contentStyle={{
                        backgroundColor: '#05070f',
                        border: '1px solid var(--primary-border)',
                        borderRadius: '0.75rem',
                        fontSize: '11px',
                        fontFamily: 'JetBrains Mono',
                        boxShadow: '0 15px 35px rgba(0,0,0,0.9), 0 0 20px var(--primary-glow-subtle)',
                        color: '#ffffff',
                      }}
                      itemStyle={{ color: '#ffffff' }}
                      labelStyle={{ color: currentConfig.primary, fontWeight: 'bold' }}
                    />
                    <Line
                      type="monotone"
                      dataKey="Avg Latency (ms)"
                      stroke={currentConfig.primary}
                      strokeWidth={2.5}
                      dot={{ r: 3, fill: currentConfig.primary }}
                      activeDot={{ r: 6, fill: '#ffffff', stroke: currentConfig.primary, strokeWidth: 2 }}
                    />
                  </LineChart>
                </ResponsiveContainer>
              ) : (
                <div className="flex items-center justify-center h-full text-slate-600 text-xs font-mono">
                  No latency benchmarks logged yet.
                </div>
              )}
            </div>
          </Card>

          {/* Model Breakdown Table */}
          <Card className="p-5">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-300 flex items-center gap-2">
                <Database className="w-4 h-4 text-slate-400" />
                Granular Model Ledger (Last {days} Days)
              </h3>
              <Badge variant="neutral">{data?.stats.length || 0} transaction records</Badge>
            </div>

            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Date</TableHead>
                  <TableHead>Provider</TableHead>
                  <TableHead>Model Target</TableHead>
                  <TableHead className="text-right">Requests</TableHead>
                  <TableHead className="text-right">Prompt Tokens</TableHead>
                  <TableHead className="text-right">Completion Tokens</TableHead>
                  <TableHead className="text-right">Latency</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data?.stats.map((s, idx) => (
                  <TableRow key={`${s.date}-${s.provider_name}-${s.model_name}-${idx}`}>
                    <TableCell className="text-slate-400 font-sans text-xs">{s.date}</TableCell>
                    <TableCell className="font-semibold text-slate-200 capitalize font-sans">
                      {s.provider_name}
                    </TableCell>
                    <TableCell className="text-slate-300 font-mono text-[11px]">{s.model_name}</TableCell>
                    <TableCell className="text-right text-slate-300">{s.total_requests}</TableCell>
                    <TableCell className="text-right text-[var(--primary)] font-semibold">
                      {s.total_input_tokens.toLocaleString()}
                    </TableCell>
                    <TableCell className="text-right text-indigo-400">
                      {s.total_output_tokens.toLocaleString()}
                    </TableCell>
                    <TableCell className="text-right text-slate-400">
                      {s.avg_latency_ms > 0 ? `${s.avg_latency_ms.toFixed(0)} ms` : '-'}
                    </TableCell>
                  </TableRow>
                ))}
                {(!data?.stats || data.stats.length === 0) && (
                  <TableRow>
                    <TableCell colSpan={7} className="py-8 text-center text-slate-600 font-sans text-xs">
                      No transactions recorded in the selected period.
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </Card>
        </>
      )}
    </div>
  );
};
