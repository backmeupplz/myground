import { useState, useCallback, useEffect, useRef } from "preact/hooks";
import {
  api,
  formatBytes,
  type AppInfo,
  type SystemStats,
  type AvailableApp,
} from "../api";
import { usePolling } from "../hooks/use-polling";
import { AppCard, getAppStatus } from "../components/app-card";
import { InstallModal } from "../components/install-modal";
import { AppPicker } from "../components/app-picker";

function RingGauge({ percent, size = 32, stroke = 3 }: { percent: number; size?: number; stroke?: number }) {
  const r = (size - stroke) / 2;
  const circ = 2 * Math.PI * r;
  const offset = circ * (1 - Math.min(100, percent) / 100);
  const color = percent > 85 ? "#ED5B5A" : percent > 60 ? "#E9BB4F" : "#92a593";
  return (
    <svg width={size} height={size} class="shrink-0 -rotate-90">
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="#1e2a50" stroke-width={stroke} />
      <circle
        cx={size / 2} cy={size / 2} r={r} fill="none"
        stroke={color} stroke-width={stroke}
        stroke-dasharray={circ} stroke-dashoffset={offset}
        stroke-linecap="round"
      />
    </svg>
  );
}

function StatsBar({ stats }: { stats: SystemStats }) {
  const ramPercent = (stats.ram_used_bytes / stats.ram_total_bytes) * 100;
  return (
    <div class="flex flex-wrap items-center gap-4 mb-6 max-w-6xl mx-auto text-xs">
      {/* CPU */}
      <div class="flex items-center gap-2.5 bg-gray-800/60 border border-gray-700/50 rounded-full px-4 py-2">
        <RingGauge percent={stats.cpu_usage_percent} size={28} stroke={2.5} />
        <div>
          <span class="text-gray-400 mr-1.5">CPU</span>
          <span class="text-white font-semibold text-sm">{stats.cpu_usage_percent.toFixed(0)}%</span>
        </div>
      </div>

      {/* RAM */}
      <div class="flex items-center gap-2.5 bg-gray-800/60 border border-gray-700/50 rounded-full px-4 py-2">
        <RingGauge percent={ramPercent} size={28} stroke={2.5} />
        <div>
          <span class="text-gray-400 mr-1.5">RAM</span>
          <span class="text-white font-semibold text-sm">{formatBytes(stats.ram_used_bytes)}</span>
          <span class="text-gray-500 ml-1">/ {formatBytes(stats.ram_total_bytes)}</span>
        </div>
      </div>

    </div>
  );
}

export function Dashboard() {
  const fetchApps = useCallback(() => api.apps(), []);
  const fetchStats = useCallback(() => api.stats(), []);
  const appsRef = useRef<AppInfo[] | null>(null);
  const pollInterval = useCallback(() => {
    const current = appsRef.current;
    if (!current) return 5000;
    const anyTransitioning = current.some((a) => {
      const s = getAppStatus(a);
      return s === "starting" || s === "health_checking";
    });
    return anyTransitioning ? 2000 : 15000;
  }, []);
  const [apps, loading, refetchApps] = usePolling<AppInfo[]>(fetchApps, pollInterval);
  const [stats] = usePolling<SystemStats>(fetchStats, 15000);

  // Keep ref in sync for adaptive polling
  useEffect(() => {
    appsRef.current = apps;
  }, [apps]);
  const [acting, setActing] = useState<string | null>(null);
  const [showPicker, setShowPicker] = useState(false);
  const [installTarget, setInstallTarget] = useState<AvailableApp | null>(
    null,
  );
  const [serverIp, setServerIp] = useState<string | undefined>(undefined);

  useEffect(() => {
    api.health().then((h) => setServerIp(h.server_ip)).catch(() => {});
  }, []);

  const doAction = async (id: string, action: "start" | "stop") => {
    setActing(id);
    try {
      switch (action) {
        case "start":
          await api.startApp(id);
          break;
        case "stop":
          await api.stopApp(id);
          break;
      }
      setTimeout(refetchApps, 1000);
    } finally {
      setActing(null);
    }
  };

  const installed = (apps ?? []).filter((s) => s.installed);

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading apps...</p>
      </div>
    );
  }

  return (
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6">
      {stats && <StatsBar stats={stats} />}

      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 max-w-6xl mx-auto">
        {installed.map((svc) => (
          <AppCard
            key={svc.id}
            app={svc}
            busy={acting === svc.id}
            onStart={() => doAction(svc.id, "start")}
            onStop={() => doAction(svc.id, "stop")}
            serverIp={serverIp}
          />
        ))}

        {/* Add app card */}
        <button
          class="bg-gray-900/50 border-2 border-dashed border-gray-700 hover:border-gray-500 rounded-lg p-5 flex flex-col items-center justify-center gap-2 transition-colors min-h-[120px] cursor-pointer"
          onClick={() => setShowPicker(true)}
        >
          <span class="text-3xl text-gray-500">+</span>
          <span class="text-sm text-gray-500">Add App</span>
        </button>
      </div>

      {showPicker && (
        <AppPicker
          onSelect={(svc) => {
            setShowPicker(false);
            setInstallTarget(svc);
          }}
          onClose={() => setShowPicker(false)}
        />
      )}

      {installTarget && (
        <InstallModal
          appId={installTarget.id}
          appName={installTarget.name}
          hasStorage={!!installTarget.has_storage}
          backupSupported={installTarget.backup_supported}
          installVariables={installTarget.install_variables}
          storageVolumes={installTarget.storage_volumes}
          onClose={() => setInstallTarget(null)}
          onInstalled={refetchApps}
        />
      )}
    </div>
  );
}
