import React, { useEffect, useMemo, useState } from 'react';
import {
  Activity,
  ArrowUpRight,
  ArrowDownRight,
  AlertOctagon,
  AlertTriangle,
  Server,
  Copy,
  Check,
  Code,
} from 'lucide-react';
import {
  ResponsiveContainer,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  CartesianGrid,
  LineChart,
  Line,
  Legend,
} from 'recharts';
import { ParsedPrometheusMetrics } from '../types/api';
import { formatBytes } from '../utils/formatters';

interface MetricSample {
  t: number;
  label: string;
  uploaded: number;
  downloaded: number;
  failures: number;
  nodes: number;
  underReplicated: number;
}

interface MetricsViewProps {
  metrics: ParsedPrometheusMetrics | null;
  loading: boolean;
  onRefresh: () => void;
  onNotify: (type: 'success' | 'error' | 'info' | 'warning', title: string, message?: string) => void;
}

export const MetricsView: React.FC<MetricsViewProps> = ({
  metrics,
  loading,
  onNotify,
}) => {
  const [copiedRaw, setCopiedRaw] = useState(false);
  const [showRaw, setShowRaw] = useState(false);
  const [history, setHistory] = useState<MetricSample[]>([]);

  useEffect(() => {
    if (!metrics || !metrics.available) return;
    const now = Date.now();
    const sample: MetricSample = {
      t: now,
      label: new Date(now).toLocaleTimeString(),
      uploaded: metrics.uploadedBytesTotal || 0,
      downloaded: metrics.downloadedBytesTotal || 0,
      failures: metrics.uploadFailuresTotal || 0,
      nodes: metrics.activeStorageNodes || 0,
      underReplicated: metrics.underReplicatedChunks || 0,
    };
    setHistory((prev) => {
      const last = prev[prev.length - 1];
      if (
        last &&
        last.uploaded === sample.uploaded &&
        last.downloaded === sample.downloaded &&
        last.failures === sample.failures &&
        last.nodes === sample.nodes &&
        last.underReplicated === sample.underReplicated
      ) {
        return prev; // avoid duplicate identical samples
      }
      const next = [...prev, sample];
      return next.slice(-30);
    });
  }, [metrics]);

  const handleCopyRaw = () => {
    if (!metrics?.rawText) return;
    navigator.clipboard.writeText(metrics.rawText);
    setCopiedRaw(true);
    setTimeout(() => setCopiedRaw(false), 2000);
    onNotify('info', 'Copied Metrics', 'Prometheus exposition text copied to clipboard.');
  };

  const chartData = [
    {
      name: 'Uploaded',
      bytes: metrics?.uploadedBytesTotal || 0,
      formatted: formatBytes(metrics?.uploadedBytesTotal || 0),
    },
    {
      name: 'Downloaded',
      bytes: metrics?.downloadedBytesTotal || 0,
      formatted: formatBytes(metrics?.downloadedBytesTotal || 0),
    },
  ];

  const historyWithDeltas = useMemo(() => {
    return history.map((s, i) => {
      const prev = i > 0 ? history[i - 1] : null;
      return {
        ...s,
        uploadedDelta: prev ? Math.max(0, s.uploaded - prev.uploaded) : 0,
        downloadedDelta: prev ? Math.max(0, s.downloaded - prev.downloaded) : 0,
      };
    });
  }, [history]);

  return (
    <div className="flex-1 flex flex-col h-full overflow-auto bg-slate-950 p-6 space-y-6">
      {/* Title */}
      <div className="flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold tracking-tight text-slate-100">
            System Telemetry & Prometheus Metrics
          </h2>
          <p className="text-xs text-slate-400 mt-0.5">
            Observability metrics scraped directly from RustyStore's <code className="text-rust-400">/metrics</code> endpoint
          </p>
        </div>

        <button
          onClick={() => setShowRaw(!showRaw)}
          className="flex items-center gap-2 px-3 py-1.5 rounded-xl text-xs font-semibold bg-slate-900 border border-slate-800 text-slate-300 hover:text-slate-100 transition-colors"
        >
          <Code className="w-3.5 h-3.5" />
          <span>{showRaw ? 'Hide Raw Metrics' : 'View Raw Exposition'}</span>
        </button>
      </div>

      {metrics && !metrics.available && (
        <div className="p-4 rounded-2xl bg-rose-500/10 border border-rose-500/30 flex items-center gap-3">
          <AlertTriangle className="w-5 h-5 text-rose-400 shrink-0" />
          <div>
            <h4 className="text-sm font-semibold text-rose-300">Live metrics unavailable</h4>
            <p className="text-xs text-rose-200/70">
              The gateway metrics endpoint could not be reached. Counters below are shown as zero; no demo values are substituted.
            </p>
          </div>
        </div>
      )}

      {/* Metrics Stat Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Uploaded Bytes */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Total Uploaded</span>
            <div className="p-2 rounded-xl bg-rust-500/10 text-rust-400">
              <ArrowUpRight className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold font-mono text-slate-100">
              {formatBytes(metrics?.uploadedBytesTotal || 0)}
            </div>
            <p className="text-[11px] text-slate-500 mt-1">rustystore_uploaded_bytes_total</p>
          </div>
        </div>

        {/* Downloaded Bytes */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Total Downloaded</span>
            <div className="p-2 rounded-xl bg-emerald-500/10 text-emerald-400">
              <ArrowDownRight className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold font-mono text-slate-100">
              {formatBytes(metrics?.downloadedBytesTotal || 0)}
            </div>
            <p className="text-[11px] text-slate-500 mt-1">rustystore_downloaded_bytes_total</p>
          </div>
        </div>

        {/* Upload Failures */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Upload Failures</span>
            <div
              className={`p-2 rounded-xl ${
                (metrics?.uploadFailuresTotal || 0) === 0
                  ? 'bg-emerald-500/10 text-emerald-400'
                  : 'bg-rose-500/10 text-rose-400'
              }`}
            >
              <AlertOctagon className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold font-mono text-slate-100">
              {metrics?.uploadFailuresTotal ?? 0}
            </div>
            <p className="text-[11px] text-slate-500 mt-1">rustystore_upload_failures_total</p>
          </div>
        </div>

        {/* Active Storage Nodes */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Active Nodes</span>
            <div className="p-2 rounded-xl bg-sky-500/10 text-sky-400">
              <Server className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold font-mono text-slate-100">
              {metrics?.activeStorageNodes ?? 0}
            </div>
            <p className="text-[11px] text-slate-500 mt-1">rustystore_active_storage_nodes</p>
          </div>
        </div>
      </div>

      {/* Throughput Chart */}
      <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Activity className="w-4 h-4 text-rust-400" />
            <h3 className="text-sm font-bold uppercase tracking-wider text-slate-300">
              Gateway Bandwidth Distribution
            </h3>
          </div>
          <span className="text-xs text-slate-500">Cumulative bytes transferred</span>
        </div>

        <div className="h-64 w-full">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={chartData} margin={{ top: 20, right: 30, left: 20, bottom: 5 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="#1e293b" />
              <XAxis dataKey="name" stroke="#64748b" tick={{ fontSize: 12 }} />
              <YAxis
                stroke="#64748b"
                tick={{ fontSize: 11 }}
                tickFormatter={(val) => formatBytes(val)}
              />
              <Tooltip
                contentStyle={{
                  backgroundColor: '#0f172a',
                  borderColor: '#334155',
                  borderRadius: '12px',
                  color: '#f8fafc',
                  fontSize: '12px',
                }}
                formatter={(value: any) => [formatBytes(Number(value)), 'Total Volume']}
              />
              <Bar dataKey="bytes" fill="#e05a47" radius={[8, 8, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Sample History */}
      <div className="p-6 rounded-2xl bg-slate-900/60 border border-slate-800/80 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Activity className="w-4 h-4 text-sky-400" />
            <h3 className="text-sm font-bold uppercase tracking-wider text-slate-300">
              Recent Telemetry Samples
            </h3>
          </div>
          <span className="text-xs text-slate-500">
            {history.length > 1
              ? `Last ${history.length} samples (client-side)`
              : 'Collecting samples over time...'}
          </span>
        </div>

        {historyWithDeltas.length > 1 ? (
          <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
            <div className="h-56 w-full">
              <div className="text-[11px] text-slate-400 mb-2">Bytes transferred per sample interval (delta)</div>
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={historyWithDeltas} margin={{ top: 10, right: 20, left: 10, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#1e293b" />
                  <XAxis dataKey="label" stroke="#64748b" tick={{ fontSize: 10 }} minTickGap={20} />
                  <YAxis stroke="#64748b" tick={{ fontSize: 10 }} tickFormatter={(v) => formatBytes(Number(v))} />
                  <Tooltip
                    contentStyle={{ backgroundColor: '#0f172a', borderColor: '#334155', borderRadius: '12px', color: '#f8fafc', fontSize: '12px' }}
                    formatter={(value: any, name: string) => [formatBytes(Number(value)), name === 'uploadedDelta' ? 'Uploaded Δ' : 'Downloaded Δ']}
                  />
                  <Legend />
                  <Line type="monotone" dataKey="uploadedDelta" name="Uploaded Δ" stroke="#e05a47" strokeWidth={2} dot={false} />
                  <Line type="monotone" dataKey="downloadedDelta" name="Downloaded Δ" stroke="#34d399" strokeWidth={2} dot={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
            <div className="h-56 w-full">
              <div className="text-[11px] text-slate-400 mb-2">Nodes, failures, and under-replicated chunks</div>
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={historyWithDeltas} margin={{ top: 10, right: 20, left: 10, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#1e293b" />
                  <XAxis dataKey="label" stroke="#64748b" tick={{ fontSize: 10 }} minTickGap={20} />
                  <YAxis stroke="#64748b" tick={{ fontSize: 10 }} allowDecimals={false} />
                  <Tooltip
                    contentStyle={{ backgroundColor: '#0f172a', borderColor: '#334155', borderRadius: '12px', color: '#f8fafc', fontSize: '12px' }}
                  />
                  <Legend />
                  <Line type="stepAfter" dataKey="nodes" name="Active nodes" stroke="#38bdf8" strokeWidth={2} dot={false} />
                  <Line type="stepAfter" dataKey="failures" name="Upload failures" stroke="#fb7185" strokeWidth={2} dot={false} />
                  <Line type="stepAfter" dataKey="underReplicated" name="Under-replicated" stroke="#fbbf24" strokeWidth={2} dot={false} />
                </LineChart>
              </ResponsiveContainer>
            </div>
          </div>
        ) : (
          <div className="h-32 flex items-center justify-center text-xs text-slate-500 border border-dashed border-slate-800 rounded-xl">
            {metrics?.available === false
              ? 'History unavailable while live metrics are unreachable.'
              : 'Waiting for additional samples to render trends...'}
          </div>
        )}
      </div>

      {/* Raw Prometheus Output Modal / Section */}
      {showRaw && (
        <div className="p-6 rounded-2xl bg-slate-900 border border-slate-800 space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">
              Raw Prometheus Exposition Text
            </span>
            <button
              onClick={handleCopyRaw}
              className="flex items-center gap-1.5 px-3 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 transition-colors"
            >
              {copiedRaw ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
              <span>{copiedRaw ? 'Copied' : 'Copy'}</span>
            </button>
          </div>
          <pre className="p-4 rounded-xl bg-slate-950 border border-slate-800 text-[11px] font-mono-code text-slate-300 overflow-auto max-h-72">
            {metrics?.rawText || 'No Prometheus metrics available'}
          </pre>
        </div>
      )}
    </div>
  );
};
