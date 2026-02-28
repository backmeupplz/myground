import { useState, useEffect, useCallback } from "preact/hooks";
import {
  api,
  formatBytes,
  type ServiceInfo,
  type SystemStats,
  type AvailableService,
} from "../api";
import { ServiceCard } from "../components/service-card";
import { InstallModal } from "../components/install-modal";
import { ServicePicker } from "../components/service-picker";

function RingGauge({ percent, size = 32, stroke = 3 }: { percent: number; size?: number; stroke?: number }) {
  const r = (size - stroke) / 2;
  const circ = 2 * Math.PI * r;
  const offset = circ * (1 - Math.min(100, percent) / 100);
  const color = percent > 85 ? "#ef4444" : percent > 60 ? "#f59e0b" : "#22c55e";
  return (
    <svg width={size} height={size} class="shrink-0 -rotate-90">
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="#374151" stroke-width={stroke} />
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
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [stats, setStats] = useState<SystemStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [acting, setActing] = useState<string | null>(null);
  const [showPicker, setShowPicker] = useState(false);
  const [installTarget, setInstallTarget] = useState<AvailableService | null>(
    null,
  );

  const fetchData = useCallback(() => {
    api
      .services()
      .then((data) => {
        setServices(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));

    api.stats().then(setStats).catch(() => {});
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000);
    return () => clearInterval(interval);
  }, [fetchData]);

  const doAction = async (id: string, action: "start" | "stop") => {
    setActing(id);
    try {
      switch (action) {
        case "start":
          await api.startService(id);
          break;
        case "stop":
          await api.stopService(id);
          break;
      }
      setTimeout(fetchData, 1000);
    } finally {
      setActing(null);
    }
  };

  const installed = services.filter((s) => s.installed);

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading services...</p>
      </div>
    );
  }

  return (
    <div class="flex-1 px-6 py-6">
      {stats && <StatsBar stats={stats} />}

      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 max-w-6xl mx-auto">
        {installed.map((svc) => (
          <ServiceCard
            key={svc.id}
            service={svc}
            busy={acting === svc.id}
            onStart={() => doAction(svc.id, "start")}
            onStop={() => doAction(svc.id, "stop")}
          />
        ))}

        {/* Add service card */}
        <button
          class="bg-gray-900/50 border-2 border-dashed border-gray-700 hover:border-gray-500 rounded-lg p-5 flex flex-col items-center justify-center gap-2 transition-colors min-h-[120px] cursor-pointer"
          onClick={() => setShowPicker(true)}
        >
          <span class="text-3xl text-gray-500">+</span>
          <span class="text-sm text-gray-500">Add Service</span>
        </button>
      </div>

      {showPicker && (
        <ServicePicker
          onSelect={(svc) => {
            setShowPicker(false);
            setInstallTarget(svc);
          }}
          onClose={() => setShowPicker(false)}
        />
      )}

      {installTarget && (
        <InstallModal
          serviceId={installTarget.id}
          serviceName={installTarget.name}
          hasStorage={!!installTarget.has_storage}
          backupSupported={installTarget.backup_supported}
          installVariables={installTarget.install_variables}
          onClose={() => setInstallTarget(null)}
          onInstalled={fetchData}
        />
      )}
    </div>
  );
}
