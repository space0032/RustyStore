import React from 'react';
import { Files, Layers, Server, Activity } from 'lucide-react';

export type ActiveTab = 'files' | 'chunks' | 'cluster' | 'metrics';

interface SidebarProps {
  activeTab: ActiveTab;
  onTabChange: (tab: ActiveTab) => void;
  fileCount?: number;
  nodeCount?: number;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeTab,
  onTabChange,
  fileCount,
  nodeCount,
}) => {
  const navItems = [
    {
      id: 'files' as ActiveTab,
      label: 'Objects & Files',
      description: 'Upload, manage, download',
      icon: Files,
      badge: fileCount !== undefined ? fileCount.toString() : undefined,
    },
    {
      id: 'chunks' as ActiveTab,
      label: 'Chunk Visualizer',
      description: '4MB chunk & replica map',
      icon: Layers,
    },
    {
      id: 'cluster' as ActiveTab,
      label: 'Cluster Topology',
      description: 'Nodes & health status',
      icon: Server,
      badge: nodeCount !== undefined ? `${nodeCount} nodes` : undefined,
    },
    {
      id: 'metrics' as ActiveTab,
      label: 'Metrics & Telemetry',
      description: 'Prometheus live counters',
      icon: Activity,
    },
  ];

  return (
    <aside className="w-64 border-r border-slate-800/80 bg-slate-950 flex flex-col justify-between shrink-0 h-[calc(100vh-4rem)]">
      <div className="p-4 space-y-1.5">
        <div className="px-3 py-2 text-[11px] font-semibold tracking-wider uppercase text-slate-500">
          Navigation
        </div>
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = activeTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onTabChange(item.id)}
              className={`w-full flex items-center justify-between p-3 rounded-xl text-left transition-all group ${
                isActive
                  ? 'bg-gradient-to-r from-rust-500/15 to-transparent text-rust-300 border border-rust-500/30'
                  : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60 border border-transparent'
              }`}
            >
              <div className="flex items-center gap-3 min-w-0">
                <div
                  className={`p-2 rounded-lg transition-colors ${
                    isActive
                      ? 'bg-rust-500 text-white shadow-md shadow-rust-500/30'
                      : 'bg-slate-900 text-slate-400 group-hover:text-slate-200'
                  }`}
                >
                  <Icon className="w-4 h-4" />
                </div>
                <div className="truncate">
                  <div className="text-sm font-semibold text-slate-100">{item.label}</div>
                  <div className="text-[11px] text-slate-500 truncate">{item.description}</div>
                </div>
              </div>
              {item.badge && (
                <span
                  className={`text-[10px] font-medium px-2 py-0.5 rounded-full ${
                    isActive
                      ? 'bg-rust-500/20 text-rust-300 border border-rust-500/30'
                      : 'bg-slate-900 text-slate-400'
                  }`}
                >
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Cluster Footer Info */}
      <div className="p-4 border-t border-slate-900">
        <div className="p-3 rounded-xl bg-slate-900/80 border border-slate-800/80">
          <div className="flex items-center justify-between text-xs mb-1">
            <span className="text-slate-400">Default Chunk Size</span>
            <span className="font-mono text-rust-400 font-semibold">4 MiB</span>
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-slate-400">Target Replicas (N)</span>
            <span className="font-mono text-rust-400 font-semibold">3</span>
          </div>
        </div>
      </div>
    </aside>
  );
};
