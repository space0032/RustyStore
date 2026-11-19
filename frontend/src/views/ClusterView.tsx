import React from 'react';
import {
  Server,
  HardDrive,
  ShieldCheck,
  ShieldAlert,
  AlertTriangle,
  Database,
  Layers,
  Activity,
  Cpu,
} from 'lucide-react';
import { ClusterStats, NodeInfo } from '../types/api';
import { formatBytes, truncateUuid } from '../utils/formatters';

interface ClusterViewProps {
  stats: ClusterStats | null;
  nodes: NodeInfo[];
  loading: boolean;
  onRefresh: () => void;
}

export const ClusterView: React.FC<ClusterViewProps> = ({ stats, nodes, loading }) => {
  const totalCapacity = stats?.total_capacity_bytes || 0;
  const totalAvailable = stats?.total_available_bytes || 0;
  const totalUsed = Math.max(0, totalCapacity - totalAvailable);
  const usedPercent = totalCapacity > 0 ? Math.round((totalUsed / totalCapacity) * 100) : 0;

  const healthyNodesCount = stats?.healthy_nodes ?? nodes.filter((n) => n.healthy).length;
  const totalNodesCount = stats?.total_nodes ?? nodes.length;
  const isAllHealthy = totalNodesCount > 0 && healthyNodesCount === totalNodesCount;

  return (
    <div className="flex-1 flex flex-col h-full overflow-auto bg-slate-950 p-6 space-y-6">
      {/* View Title */}
      <div>
        <h2 className="text-xl font-bold tracking-tight text-slate-100">Cluster & Node Topology</h2>
        <p className="text-xs text-slate-400 mt-0.5">
          Real-time cluster storage utilization, health leases, and distributed storage nodes
        </p>
      </div>

      {/* Under-replication Alert Banner */}
      {stats && stats.under_replicated_chunks > 0 && (
        <div className="p-4 rounded-2xl bg-amber-500/10 border border-amber-500/30 flex items-center justify-between gap-4 animate-pulse">
          <div className="flex items-center gap-3">
            <div className="p-2 bg-amber-500/20 text-amber-400 rounded-xl">
              <AlertTriangle className="w-5 h-5" />
            </div>
            <div>
              <h4 className="text-sm font-semibold text-amber-300">Under-Replicated Chunks Detected</h4>
              <p className="text-xs text-amber-200/70">
                {stats.under_replicated_chunks} chunk(s) currently have fewer than the target replication factor (N=3).
                The Coordinator background recovery loop is re-replicating chunks to healthy nodes.
              </p>
            </div>
          </div>
          <span className="px-3 py-1 rounded-full text-xs font-bold bg-amber-500 text-slate-950 font-mono">
            {stats.under_replicated_chunks} Chunks
          </span>
        </div>
      )}

      {/* Cluster Overview Grid */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Node Health Card */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Node Health</span>
            <div
              className={`p-2 rounded-xl ${
                isAllHealthy ? 'bg-emerald-500/10 text-emerald-400' : 'bg-rose-500/10 text-rose-400'
              }`}
            >
              <Server className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold text-slate-100 flex items-baseline gap-2">
              <span>{healthyNodesCount}</span>
              <span className="text-sm font-normal text-slate-400">/ {totalNodesCount} Online</span>
            </div>
            <p className="text-[11px] text-slate-500 mt-1">
              {isAllHealthy ? 'All storage nodes heartbeat active' : 'Degraded storage node detected'}
            </p>
          </div>
        </div>

        {/* Storage Pool Card */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Storage Pool</span>
            <div className="p-2 rounded-xl bg-rust-500/10 text-rust-400">
              <HardDrive className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold text-slate-100 font-mono">
              {formatBytes(totalUsed)}
              <span className="text-xs font-normal text-slate-400 font-sans ml-1.5">
                used of {formatBytes(totalCapacity)}
              </span>
            </div>
            {/* Storage Progress Bar */}
            <div className="w-full h-1.5 rounded-full bg-slate-800 mt-2 overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-rust-500 to-amber-500 transition-all duration-300"
                style={{ width: `${Math.min(100, Math.max(3, usedPercent))}%` }}
              />
            </div>
          </div>
        </div>

        {/* Files Committed Card */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Objects Stored</span>
            <div className="p-2 rounded-xl bg-sky-500/10 text-sky-400">
              <Database className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold text-slate-100">
              {stats?.total_files ?? '-'}
              <span className="text-xs font-normal text-slate-400 font-sans ml-1.5">files</span>
            </div>
            <p className="text-[11px] text-slate-500 mt-1">Tracked in PostgreSQL metadata</p>
          </div>
        </div>

        {/* Total Chunks Card */}
        <div className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-col justify-between space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Total Chunks</span>
            <div className="p-2 rounded-xl bg-purple-500/10 text-purple-400">
              <Layers className="w-4 h-4" />
            </div>
          </div>
          <div>
            <div className="text-2xl font-bold text-slate-100">
              {stats?.total_chunks ?? '-'}
              <span className="text-xs font-normal text-slate-400 font-sans ml-1.5">chunks</span>
            </div>
            <p className="text-[11px] text-slate-500 mt-1">Hashed & placed with RF=3</p>
          </div>
        </div>
      </div>

      {/* Storage Nodes List */}
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Server className="w-4 h-4 text-rust-400" />
            <h3 className="text-sm font-bold uppercase tracking-wider text-slate-300">
              Storage Nodes ({nodes.length})
            </h3>
          </div>
          <span className="text-xs text-slate-500">Communicates with Coordinator via Tonic gRPC</span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
          {nodes.map((node, index) => {
            const used = Math.max(0, node.capacity_bytes - node.available_bytes);
            const pct =
              node.capacity_bytes > 0
                ? Math.round((used / node.capacity_bytes) * 100)
                : 0;

            return (
              <div
                key={node.node_id}
                className="p-5 rounded-2xl bg-slate-900/60 border border-slate-800/80 flex flex-col justify-between space-y-4 hover:border-slate-700 transition-all"
              >
                {/* Node Header */}
                <div className="flex items-start justify-between">
                  <div className="flex items-center gap-3">
                    <div
                      className={`p-2.5 rounded-xl ${
                        node.healthy
                          ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                          : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                      }`}
                    >
                      <Server className="w-5 h-5" />
                    </div>
                    <div>
                      <h4 className="font-semibold text-slate-100">Storage Node #{index + 1}</h4>
                      <span className="font-mono text-[11px] text-slate-400">
                        {node.grpc_addr || node.addr}
                      </span>
                    </div>
                  </div>

                  <span
                    className={`px-2.5 py-1 rounded-full text-[10px] font-bold uppercase tracking-wider border ${
                      node.healthy
                        ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                        : 'bg-rose-500/10 text-rose-400 border-rose-500/20'
                    }`}
                  >
                    {node.healthy ? 'Healthy' : 'Offline'}
                  </span>
                </div>

                {/* Node Stats */}
                <div className="space-y-2 text-xs">
                  <div className="flex justify-between text-slate-400">
                    <span>Disk Utilization</span>
                    <span className="font-mono text-slate-200">{pct}%</span>
                  </div>
                  <div className="w-full h-2 rounded-full bg-slate-800 overflow-hidden">
                    <div
                      className="h-full bg-rust-500 rounded-full transition-all"
                      style={{ width: `${Math.max(4, pct)}%` }}
                    />
                  </div>
                  <div className="flex justify-between text-[11px] text-slate-500 font-mono">
                    <span>{formatBytes(used)} used</span>
                    <span>{formatBytes(node.capacity_bytes)}</span>
                  </div>
                </div>

                {/* Node UUID */}
                <div className="pt-3 border-t border-slate-800/80 flex items-center justify-between text-[11px]">
                  <span className="text-slate-500">Node ID</span>
                  <code className="text-slate-400 font-mono-code">{truncateUuid(node.node_id)}</code>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
