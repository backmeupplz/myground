import { useState, useEffect, useCallback } from "preact/hooks";
import { api, type ServiceInfo } from "../api";
import { ServiceCard } from "../components/service-card";
import { InstallModal } from "../components/install-modal";

export function Dashboard() {
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [acting, setActing] = useState<string | null>(null);
  const [installTarget, setInstallTarget] = useState<ServiceInfo | null>(null);

  const fetchServices = useCallback(() => {
    api
      .services()
      .then((data) => {
        setServices(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  useEffect(() => {
    fetchServices();
    const interval = setInterval(fetchServices, 5000);
    return () => clearInterval(interval);
  }, [fetchServices]);

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
      setTimeout(fetchServices, 1000);
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
          onClose={() => setInstallTarget(null)}
          onInstalled={fetchServices}
        />
      )}
    </div>
  );
}
