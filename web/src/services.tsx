import { useState, useEffect, useCallback } from "preact/hooks";

export interface ContainerStatus {
  name: string;
  state: string;
  status: string;
}

export interface StorageVolumeStatus {
  name: string;
  container_path: string;
  host_path: string;
  disk_available_bytes: number | null;
}

export interface ServiceInfo {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  installed: boolean;
  containers: ContainerStatus[];
  storage: StorageVolumeStatus[];
}

function getServiceStatus(
  service: ServiceInfo,
): "running" | "stopped" | "not_installed" {
  if (!service.installed) return "not_installed";
  const running = service.containers.some((c) => c.state === "running");
  return running ? "running" : "stopped";
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    running: "bg-green-500/20 text-green-400",
    stopped: "bg-yellow-500/20 text-yellow-400",
    not_installed: "bg-gray-500/20 text-gray-400",
  };

  const labels: Record<string, string> = {
    running: "Running",
    stopped: "Stopped",
    not_installed: "Not Installed",
  };

  return (
    <span class={`px-2 py-0.5 rounded text-xs font-medium ${colors[status]}`}>
      {labels[status]}
    </span>
  );
}

export function ServiceList() {
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [acting, setActing] = useState<string | null>(null);

  const fetchServices = useCallback(() => {
    fetch("/api/services")
      .then((r) => r.json())
      .then((data) => {
        setServices(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  useEffect(() => {
    fetchServices();
  }, [fetchServices]);

  const doAction = async (id: string, action: string) => {
    setActing(id);
    try {
      const method = action === "remove" ? "DELETE" : "POST";
      const url =
        action === "remove"
          ? `/api/services/${id}`
          : `/api/services/${id}/${action}`;
      await fetch(url, { method });
      // Brief delay for containers to change state
      setTimeout(fetchServices, 1000);
    } finally {
      setActing(null);
    }
  };

  if (loading) {
    return <p class="text-gray-500">Loading services...</p>;
  }

  return (
    <div class="w-full max-w-2xl space-y-3">
      {services.map((svc) => {
        const status = getServiceStatus(svc);
        const busy = acting === svc.id;

        return (
          <div
            key={svc.id}
            class="bg-gray-900 rounded-lg p-4 flex items-center justify-between"
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-1">
                <h3 class="font-semibold text-gray-100">{svc.name}</h3>
                <StatusBadge status={status} />
              </div>
              <p class="text-sm text-gray-400 truncate">{svc.description}</p>
              {svc.installed && svc.storage.length > 0 && (
                <div class="mt-2 space-y-1">
                  {svc.storage.map((vol) => (
                    <p
                      key={vol.name}
                      class="text-xs text-gray-500 font-mono truncate"
                    >
                      {vol.name}: {vol.host_path || "not set"}
                    </p>
                  ))}
                </div>
              )}
            </div>

            <div class="flex gap-2 ml-4 shrink-0">
              {status === "not_installed" && (
                <button
                  class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
                  disabled={busy}
                  onClick={() => doAction(svc.id, "install")}
                >
                  Install
                </button>
              )}
              {status === "stopped" && (
                <>
                  <button
                    class="px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white text-sm rounded disabled:opacity-50"
                    disabled={busy}
                    onClick={() => doAction(svc.id, "start")}
                  >
                    Start
                  </button>
                  <button
                    class="px-3 py-1.5 bg-red-600/80 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50"
                    disabled={busy}
                    onClick={() => doAction(svc.id, "remove")}
                  >
                    Remove
                  </button>
                </>
              )}
              {status === "running" && (
                <button
                  class="px-3 py-1.5 bg-yellow-600 hover:bg-yellow-500 text-white text-sm rounded disabled:opacity-50"
                  disabled={busy}
                  onClick={() => doAction(svc.id, "stop")}
                >
                  Stop
                </button>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
