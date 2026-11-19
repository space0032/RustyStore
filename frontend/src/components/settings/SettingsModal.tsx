import React, { useEffect, useState } from 'react';
import { X, Sliders, Key, Server, RefreshCw, Sparkles } from 'lucide-react';
import { AppSettings } from '../../services/api';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  settings: AppSettings;
  onSave: (newSettings: AppSettings) => void;
}

export const SettingsModal: React.FC<SettingsModalProps> = ({
  isOpen,
  onClose,
  settings,
  onSave,
}) => {
  const [form, setForm] = useState<AppSettings>({ ...settings });

  // The modal stays mounted; refresh the form whenever it opens or settings
  // are changed elsewhere (e.g. the navbar Demo Mode toggle).
  useEffect(() => {
    if (isOpen) setForm({ ...settings });
  }, [isOpen, settings]);

  if (!isOpen) return null;

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSave(form);
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200">
      <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-800">
          <div className="flex items-center gap-2.5">
            <div className="p-2 bg-rust-500/10 text-rust-400 rounded-lg">
              <Sliders className="w-5 h-5" />
            </div>
            <div>
              <h3 className="font-semibold text-slate-100">Console Settings</h3>
              <p className="text-xs text-slate-400">Configure connection and development parameters</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 p-1.5 rounded-lg hover:bg-slate-800 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-6 space-y-5">
          {/* Demo Mode Toggle */}
          <div className="flex items-center justify-between p-4 rounded-xl bg-slate-800/60 border border-slate-700/60">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-amber-500/10 text-amber-400 rounded-lg">
                <Sparkles className="w-5 h-5" />
              </div>
              <div>
                <span className="text-sm font-medium text-slate-200">Demo / Mock Mode</span>
                <p className="text-xs text-slate-400">Explore console with synthetic cluster data</p>
              </div>
            </div>
            <label className="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                checked={form.demoMode}
                onChange={(e) => setForm({ ...form, demoMode: e.target.checked })}
                className="sr-only peer"
              />
              <div className="w-11 h-6 bg-slate-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-slate-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-rust-500"></div>
            </label>
          </div>

          {/* Gateway Endpoint */}
          <div className="space-y-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-1.5">
              <Server className="w-3.5 h-3.5 text-rust-400" />
              Gateway Base URL
            </label>
            <input
              type="text"
              value={form.baseUrl}
              onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
              placeholder="Leave empty for default (/api or Vite proxy)"
              className="w-full px-3.5 py-2.5 rounded-xl bg-slate-950 border border-slate-800 text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-rust-500 transition-colors"
            />
            <p className="text-[11px] text-slate-500">
              Default uses Vite proxy to forward requests to <code className="text-rust-400">http://127.0.0.1:8080</code>.
            </p>
          </div>

          {/* API Key / Token */}
          <div className="space-y-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-1.5">
              <Key className="w-3.5 h-3.5 text-rust-400" />
              API Key / Bearer Token
            </label>
            <input
              type="password"
              value={form.apiKey}
              onChange={(e) => setForm({ ...form, apiKey: e.target.value })}
              placeholder="Required only if REQUIRE_AUTH=true"
              className="w-full px-3.5 py-2.5 rounded-xl bg-slate-950 border border-slate-800 text-sm text-slate-100 placeholder-slate-500 focus:outline-none focus:border-rust-500 transition-colors"
            />
            <p className="text-[11px] text-slate-500">
              Passed via <code className="text-slate-400">Authorization: Bearer &lt;token&gt;</code> header.
            </p>
          </div>

          {/* Polling Interval */}
          <div className="space-y-1.5">
            <label className="text-xs font-semibold uppercase tracking-wider text-slate-400 flex items-center gap-1.5">
              <RefreshCw className="w-3.5 h-3.5 text-rust-400" />
              Telemetry Auto-Refresh Interval
            </label>
            <select
              value={form.pollIntervalMs}
              onChange={(e) => setForm({ ...form, pollIntervalMs: parseInt(e.target.value) })}
              className="w-full px-3.5 py-2.5 rounded-xl bg-slate-950 border border-slate-800 text-sm text-slate-100 focus:outline-none focus:border-rust-500 transition-colors"
            >
              <option value={2000}>Every 2 seconds (Fast)</option>
              <option value={5000}>Every 5 seconds (Recommended)</option>
              <option value={10000}>Every 10 seconds</option>
              <option value={30000}>Every 30 seconds</option>
              <option value={0}>Disabled (Manual refresh only)</option>
            </select>
          </div>

          <div className="flex justify-end gap-3 pt-3 border-t border-slate-800">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-xl text-sm font-medium text-slate-300 hover:bg-slate-800 transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-5 py-2 rounded-xl text-sm font-semibold bg-rust-500 hover:bg-rust-600 text-white shadow-lg shadow-rust-500/25 transition-all"
            >
              Save Configuration
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
