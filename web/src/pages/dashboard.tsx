import { useState, useEffect, useCallback } from "preact/hooks";
import { api, formatBytes, type ServiceInfo, type SystemStats } from "../api";
import { ServiceCard } from "../components/service-card";
import { InstallModal } from "../components/install-modal";

function StatsBar({ stats }: { stats: SystemStats }) {
  return (
    <div class="bg-gray-800 border border-gray-700 rounded-lg p-4 mb-6 max-w-6xl mx-auto">
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 text-sm">
        {/* CPU */}
        <div>
          <div class="text-gray-400 mb-1">CPU</div>
          <div class="text-white font-medium">
            {stats.cpu_usage_percent.toFixed(1)}%
            <span class="text-gray-500 ml-2">
              {stats.cpu_count} cores
            </span>
          </div>
          <div class="text-gray-500 text-xs truncate">{stats.cpu_brand}</div>
        </div>

        {/* RAM */}
        <div>
          <div class="text-gray-400 mb-1">RAM</div>
          <div class="text-white font-medium">
            {formatBytes(stats.ram_used_bytes)}
            <span class="text-gray-500 ml-1">
              / {formatBytes(stats.ram_total_bytes)}
            </span>
          </div>
          <div class="w-full bg-gray-700 rounded-full h-1.5 mt-1">
            <div
              class="bg-amber-500 h-1.5 rounded-full"
              style={{
                width: `${Math.min(100, (stats.ram_used_bytes / stats.ram_total_bytes) * 100)}%`,
              }}
            />
          </div>
        </div>

        {/* GPU (if any) */}
        {stats.gpus.map((gpu, i) => (
          <div key={i}>
            <div class="text-gray-400 mb-1">GPU{stats.gpus.length > 1 ? ` ${i + 1}` : ""}</div>
            <div class="text-white font-medium truncate">{gpu.name}</div>
            <div class="text-gray-500 text-xs">
              {gpu.utilization_percent != null && `${gpu.utilization_percent}%`}
              {gpu.memory_used_mb != null && gpu.memory_total_mb != null && (
                <span class="ml-2">
                  VRAM {gpu.memory_used_mb} / {gpu.memory_total_mb} MB
                </span>
              )}
              {gpu.temperature_celsius != null && (
                <span class="ml-2">{gpu.temperature_celsius}°C</span>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Dashboard() {
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [stats, setStats] = useState<SystemStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [acting, setActing] = useState<string | null>(null);
  const [installTarget, setInstallTarget] = useState<ServiceInfo | null>(null);

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
        {services.map((svc) => (
          <ServiceCard
            key={svc.id}
            service={svc}
            busy={acting === svc.id}
            onInstall={() => setInstallTarget(svc)}
            onStart={() => doAction(svc.id, "start")}
            onStop={() => doAction(svc.id, "stop")}
          />
        ))}
      </div>

      {installTarget && (
        <InstallModal
          serviceId={installTarget.id}
          serviceName={installTarget.name}
          hasStorage={installTarget.has_storage}
          backupSupported={installTarget.backup_supported}
          installVariables={installTarget.install_variables}
          onClose={() => setInstallTarget(null)}
          onInstalled={fetchData}
        />
      )}
    </div>
  );
}
