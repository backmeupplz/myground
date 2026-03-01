import { route } from "preact-router";
import type { ServiceInfo } from "../api";

export type ServiceStatus = "running" | "stopped" | "not_installed";

export function getServiceStatus(service: ServiceInfo): ServiceStatus {
  if (!service.installed) return "not_installed";
  const running = service.containers.some((c) => c.state === "running");
  return running ? "running" : "stopped";
}

export const statusColors: Record<ServiceStatus, string> = {
  running: "text-green-400",
  stopped: "text-yellow-400",
  not_installed: "text-gray-400",
};

export const statusLabels: Record<ServiceStatus, string> = {
  running: "Running",
  stopped: "Stopped",
  not_installed: "Not Installed",
};

const badgeStyles: Record<ServiceStatus, string> = {
  running: "bg-green-500/20 text-green-400",
  stopped: "bg-yellow-500/20 text-yellow-400",
  not_installed: "bg-gray-500/20 text-gray-400",
};

function StatusBadge({ status }: { status: ServiceStatus }) {

  return (
    <span class={`px-2 py-0.5 rounded text-xs font-medium ${badgeStyles[status]}`}>
      {statusLabels[status]}
    </span>
  );
}

interface Props {
  service: ServiceInfo;
  onStart: () => void;
  onStop: () => void;
  busy: boolean;
}

export function ServiceCard({
  service,
  onStart,
  onStop,
  busy,
}: Props) {
  const status = getServiceStatus(service);

  const handleOpen = (e: Event) => {
    e.stopPropagation();
    if (service.port) {
      window.open(
        `http://${window.location.hostname}:${service.port}`,
        "_blank",
      );
    }
  };

  return (
    <div class="bg-gray-900 rounded-lg p-5 flex flex-col gap-3 transition-colors">
      <div class="flex items-start justify-between gap-2">
        <div class="min-w-0">
          <div class="flex items-center gap-2 mb-1">
            <h3 class="font-semibold text-gray-100 truncate">
              {service.name}
            </h3>
            <StatusBadge status={status} />
          </div>
          <p class="text-sm text-gray-400 line-clamp-2">
            {service.description}
          </p>
        </div>
      </div>

      <div class="flex gap-2 mt-auto pt-1">
        {status === "running" && service.port && (
          <button
            class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
            onClick={handleOpen}
          >
            Open
          </button>
        )}
        {status === "running" && (
          <>
            <button
              class="px-3 py-1.5 bg-yellow-600 hover:bg-yellow-500 text-white text-sm rounded disabled:opacity-50"
              disabled={busy}
              onClick={onStop}
            >
              Stop
            </button>
            <button
              class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-white text-sm rounded"
              onClick={() => route(`/service/${service.id}`)}
            >
              Manage
            </button>
          </>
        )}
        {status === "stopped" && (
          <>
            <button
              class="px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white text-sm rounded disabled:opacity-50"
              disabled={busy}
              onClick={onStart}
            >
              Start
            </button>
            <button
              class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-white text-sm rounded"
              onClick={() => route(`/service/${service.id}`)}
            >
              Manage
            </button>
          </>
        )}
      </div>
    </div>
  );
}
