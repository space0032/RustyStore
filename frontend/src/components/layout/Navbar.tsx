import React from 'react';
import { Database, ShieldCheck, ShieldAlert, Sparkles, Sliders, RefreshCw } from 'lucide-react';
import { ReadinessResponse } from '../../types/api';
import { AppSettings } from '../../services/api';

interface NavbarProps {
  readiness?: ReadinessResponse;
  isFetching: boolean;
  onRefresh: () => void;
  settings: AppSettings;
  onOpenSettings: () => void;
  onToggleDemo: () => void;
}

export const Navbar: React.FC<NavbarProps> = ({
  readiness,
  isFetching,
  onRefresh,
  settings,
  onOpenSettings,
  onToggleDemo,
}) => {
  const isReady = readiness?.status === 'ready' || settings.demoMode;

  return (
    <header className="h-16 border-b border-slate-800/80 bg-slate-950/80 backdrop-blur-xl px-6 flex items-center justify-between sticky top-0 z-30">
      {/* Brand Logo & Title */}
      <div className="flex items-center gap-3">
        <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-rust-500 to-rust-700 flex items-center justify-center shadow-lg shadow-rust-500/20 text-white font-bold">
          <Database className="w-5 h-5 text-white" />
        </div>
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-base font-bold tracking-tight text-slate-100">RustyStore</h1>
            <span className="text-[10px] font-semibold tracking-wider uppercase px-2 py-0.5 rounded-full bg-rust-500/10 text-rust-400 border border-rust-500/20">
              Console
            </span>
          </div>
          <p className="text-[11px] text-slate-400">Distributed File Storage System</p>
        </div>
      </div>

      {/* Status Bar & Actions */}
      <div className="flex items-center gap-3.5">
        {/* Cluster Status Pill */}
        <div
          className={`flex items-center gap-2 px-3 py-1.5 rounded-full text-xs font-medium border transition-colors ${
            isReady
              ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
              : 'bg-rose-500/10 text-rose-400 border-rose-500/20'
          }`}
          title={isReady ? 'Coordinator and Gateway ready' : readiness?.reason || 'Service unreachable'}
        >
          {isReady ? (
            <ShieldCheck className="w-4 h-4 text-emerald-400 animate-pulse" />
          ) : (
            <ShieldAlert className="w-4 h-4 text-rose-400" />
          )}
          <span>{isReady ? (settings.demoMode ? 'Demo Cluster' : 'Cluster Online') : 'Cluster Offline'}</span>
        </div>

        {/* Demo Mode Toggle Badge */}
        <button
          onClick={onToggleDemo}
          className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold border transition-all ${
            settings.demoMode
              ? 'bg-amber-500/20 text-amber-300 border-amber-500/40 shadow-lg shadow-amber-500/10'
              : 'bg-slate-900 text-slate-400 border-slate-800 hover:text-slate-200'
          }`}
        >
          <Sparkles className="w-3.5 h-3.5" />
          <span>Demo Mode {settings.demoMode ? 'ON' : 'OFF'}</span>
        </button>

        {/* Refresh Button */}
        <button
          onClick={onRefresh}
          disabled={isFetching}
          className="p-2 rounded-xl text-slate-400 hover:text-slate-200 hover:bg-slate-900 border border-slate-800 transition-colors disabled:opacity-50"
          title="Manual refresh"
        >
          <RefreshCw className={`w-4 h-4 ${isFetching ? 'animate-spin text-rust-400' : ''}`} />
        </button>

        {/* Settings Button */}
        <button
          onClick={onOpenSettings}
          className="p-2 rounded-xl text-slate-400 hover:text-slate-200 hover:bg-slate-900 border border-slate-800 transition-colors"
          title="Settings & Connection"
        >
          <Sliders className="w-4 h-4" />
        </button>
      </div>
    </header>
  );
};
