import { route } from "preact-router";
import type { AppInfo } from "../api";
import { isReady, isHealthChecking } from "../api";
import { AppIcon } from "./app-icon";

export type AppStatus = "running" | "stopped" | "not_installed" | "starting" | "health_checking";

export function getAppStatus(app: AppInfo): AppStatus {
  if (app.deploying) return "starting";
  if (!app.installed) return "not_installed";
  const anyRunning = app.containers.some((c) => c.state === "running");
  if (!anyRunning) return "stopped";
  if (isHealthChecking(app.containers)) return "health_checking";
  return isReady(app.containers, app.has_health_check) ? "running" : "starting";
}

export const statusColors: Record<AppStatus, string> = {
  running: "text-green-400",
  stopped: "text-yellow-400",
  not_installed: "text-gray-400",
  starting: "text-blue-400",
  health_checking: "text-cyan-400",
};

export const statusLabels: Record<AppStatus, string> = {
  running: "Running",
  stopped: "Stopped",
  not_installed: "Not Installed",
  starting: "Starting...",
  health_checking: "Initializing...",
};

const badgeStyles: Record<AppStatus, string> = {
  running: "bg-green-500/20 text-green-400",
  stopped: "bg-yellow-500/20 text-yellow-400",
  not_installed: "bg-gray-500/20 text-gray-400",
  starting: "bg-blue-500/20 text-blue-400",
  health_checking: "bg-cyan-500/20 text-cyan-400",
};

function StatusBadge({ status }: { status: AppStatus }) {
  return (
    <span class={`px-2 py-0.5 rounded text-xs font-medium inline-flex items-center gap-1 ${badgeStyles[status]}`}>
      {(status === "starting" || status === "health_checking") && (
        <svg class="animate-spin h-3 w-3" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
      )}
      {statusLabels[status]}
    </span>
  );
}

interface Props {
  app: AppInfo;
  onStart: () => void;
  onStop: () => void;
  busy: boolean;
  serverIp?: string;
}

function getOpenInfo(app: AppInfo, serverIp?: string): { url: string; label: string } | null {
  if (app.domain_url) return { url: app.domain_url, label: "Open" };
  if (app.tailscale_url && !app.tailscale_disabled) return { url: app.tailscale_url, label: "Open via Tailnet" };
  if (app.lan_accessible && app.port && serverIp) return { url: `http://${serverIp}:${app.port}${app.web_path || ""}`, label: "Open via LAN" };
  return null;
}

export function AppCard({
  app,
  onStart,
  onStop,
  busy,
  serverIp,
}: Props) {
  const status = getAppStatus(app);
  const openInfo = (status === "running" || status === "health_checking") ? getOpenInfo(app, serverIp) : null;

  // For "starting" status, show actual container status text if available
  const startingText = (() => {
    if (status !== "starting") return "";
    const running = app.containers.find((c) => c.state === "running");
    if (running) return running.status;
    const pulling = app.containers.find((c) => c.state === "created");
    if (pulling) return "Pulling images...";
    return "Starting...";
  })();

  return (
    <div class="bg-gray-900 rounded-lg p-5 flex flex-col gap-2 transition-colors">
      <div>
        <div class="flex items-center gap-2 mb-1">
          <AppIcon id={app.id} class="w-4 h-4 shrink-0" />
          <h3 class="font-semibold text-gray-100 truncate">
            {app.name}
          </h3>
          <StatusBadge status={status} />
          {app.update_available && (
            <span class="px-2 py-0.5 rounded text-xs font-medium bg-blue-500/20 text-blue-400">
              Update
            </span>
          )}
        </div>
        <p class="text-sm text-gray-400 line-clamp-2">
          {app.description}
        </p>
      </div>

      <div class="flex gap-1.5 mt-auto pt-1">
        {status === "starting" && (
          <>
            <span class="text-xs text-blue-400 truncate min-w-0">
              {startingText}
            </span>
            <button
              class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-white text-xs rounded ml-auto whitespace-nowrap"
              onClick={() => route(`/app/${app.id}`)}
            >
              Manage
            </button>
          </>
        )}
        {status === "health_checking" && (
          <>
            {openInfo && (
              <button
                class="px-2 py-1 bg-cyan-700 hover:bg-cyan-600 text-white text-xs rounded whitespace-nowrap"
                onClick={(e: Event) => {
                  e.stopPropagation();
                  window.open(openInfo.url, "_blank");
                }}
              >
                Try Open
              </button>
            )}
            <button
              class="px-2 py-1 bg-yellow-600 hover:bg-yellow-500 text-white text-xs rounded disabled:opacity-50 whitespace-nowrap"
              disabled={busy}
              onClick={onStop}
            >
              Stop
            </button>
            <button
              class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-white text-xs rounded ml-auto whitespace-nowrap"
              onClick={() => route(`/app/${app.id}`)}
            >
              Manage
            </button>
          </>
        )}
        {status === "running" && openInfo && (
          <button
            class="px-2 py-1 bg-blue-600 hover:bg-blue-500 text-white text-xs rounded whitespace-nowrap"
            onClick={(e: Event) => {
              e.stopPropagation();
              window.open(openInfo.url, "_blank");
            }}
          >
            {openInfo.label}
          </button>
        )}
        {status === "running" && (
          <>
            <button
              class="px-2 py-1 bg-yellow-600 hover:bg-yellow-500 text-white text-xs rounded disabled:opacity-50 whitespace-nowrap"
              disabled={busy}
              onClick={onStop}
            >
              Stop
            </button>
            <button
              class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-white text-xs rounded whitespace-nowrap"
              onClick={() => route(`/app/${app.id}`)}
            >
              Manage
            </button>
          </>
        )}
        {status === "stopped" && (
          <>
            <button
              class="px-2 py-1 bg-green-600 hover:bg-green-500 text-white text-xs rounded disabled:opacity-50 whitespace-nowrap"
              disabled={busy}
              onClick={onStart}
            >
              Start
            </button>
            <button
              class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-white text-xs rounded whitespace-nowrap"
              onClick={() => route(`/app/${app.id}`)}
            >
              Manage
            </button>
          </>
        )}
      </div>
    </div>
  );
}
