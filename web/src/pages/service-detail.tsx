import { useState, useEffect, useCallback } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatBytes,
  type ServiceInfo,
  type StorageVolumeStatus,
} from "../api";
import { getServiceStatus } from "../components/service-card";
import { LogViewer } from "../components/log-viewer";
import { BackupForm } from "../components/backup-form";
import { PathPicker } from "../components/path-picker";

interface Props {
  id?: string;
}

function StorageRow({
  vol,
  serviceId,
  onUpdated,
}: {
  vol: StorageVolumeStatus;
  serviceId: string;
  onUpdated: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);

  const handlePathSelect = async (path: string) => {
    setSaving(true);
    try {
      await api.updateStorage(serviceId, { [vol.name]: path });
      onUpdated();
      setEditing(false);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="bg-gray-900 rounded-lg px-4 py-3">
      <div class="flex items-center justify-between">
        <div class="min-w-0 mr-3">
          <span class="text-gray-200 capitalize">{vol.name}</span>
          {vol.host_path && (
            <p class="text-xs text-gray-500 font-mono truncate">
              {vol.host_path}
            </p>
          )}
        </div>
        <div class="flex items-center gap-3 shrink-0">
          {vol.disk_available_bytes != null && (
            <span class="text-sm text-gray-400">
              {formatBytes(vol.disk_available_bytes)} free
            </span>
          )}
          <button
            class="text-xs text-blue-400 hover:text-blue-300"
            onClick={() => setEditing(!editing)}
          >
            {editing ? "Cancel" : "Change"}
          </button>
        </div>
      </div>
      {editing && (
        <div class="mt-3 space-y-2">
          <p class="text-xs text-yellow-400">
            Changing storage won't move existing files. Restart to apply.
          </p>
          {saving ? (
            <p class="text-gray-500 text-sm">Saving...</p>
          ) : (
            <PathPicker
              initialPath={vol.host_path || "/"}
              onSelect={handlePathSelect}
              onCancel={() => setEditing(false)}
            />
          )}
        </div>
      )}
    </div>
  );
}

function ConfigRow({
  label,
  value,
  isPassword,
}: {
  label: string;
  value: string;
  isPassword: boolean;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(value).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div class="bg-gray-900 rounded-lg px-4 py-3 flex items-center justify-between">
      <div class="min-w-0 mr-3">
        <span class="text-gray-200">{label}</span>
        <p class="text-xs text-gray-500 font-mono truncate">
          {isPassword ? "\u2022".repeat(8) : value}
        </p>
      </div>
      <button
        class="text-xs text-blue-400 hover:text-blue-300 shrink-0"
        onClick={handleCopy}
      >
        {copied ? "Copied!" : "Copy"}
      </button>
    </div>
  );
}

export function ServiceDetail({ id }: Props) {
  const [service, setService] = useState<ServiceInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [acting, setActing] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);

  const fetchService = useCallback(() => {
    if (!id) return;
    api
      .services()
      .then((all) => {
        const svc = all.find((s) => s.id === id) ?? null;
        setService(svc);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [id]);

  useEffect(() => {
    fetchService();
    const interval = setInterval(fetchService, 5000);
    return () => clearInterval(interval);
  }, [fetchService]);

  const doAction = async (action: "start" | "stop") => {
    if (!id) return;
    setActing(true);
    try {
      if (action === "start") await api.startService(id);
      else await api.stopService(id);
      setTimeout(fetchService, 1000);
    } finally {
      setActing(false);
    }
  };

  const handleRemove = async () => {
    if (!id) return;
    setActing(true);
    try {
      await api.removeService(id);
      route("/");
    } finally {
      setActing(false);
    }
  };

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  if (!service) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Service not found.</p>
      </div>
    );
  }

  const status = getServiceStatus(service);

  const statusColors = {
    running: "text-green-400",
    stopped: "text-yellow-400",
    not_installed: "text-gray-400",
  };

  const statusLabels = {
    running: "Running",
    stopped: "Stopped",
    not_installed: "Not Installed",
  };

  return (
    <div class="flex-1 px-6 py-6 max-w-4xl mx-auto w-full space-y-6">
      {/* Header */}
      <div class="flex items-center justify-between flex-wrap gap-4">
        <div>
          <div class="flex items-center gap-3">
            <h1 class="text-2xl font-bold text-gray-100">{service.name}</h1>
            <span class={`text-sm font-medium ${statusColors[status]}`}>
              {statusLabels[status]}
            </span>
          </div>
          <p class="text-gray-400 mt-1">{service.description}</p>
        </div>
        <div class="flex gap-2">
          {status === "running" && service.port && (
            <button
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
              onClick={() =>
                window.open(
                  `http://${window.location.hostname}:${service.port}`,
                  "_blank",
                )
              }
            >
              Open
            </button>
          )}
          {status === "running" && (
            <button
              class="px-4 py-2 bg-yellow-600 hover:bg-yellow-500 text-white text-sm rounded disabled:opacity-50"
              disabled={acting}
              onClick={() => doAction("stop")}
            >
              Stop
            </button>
          )}
          {status === "stopped" && (
            <button
              class="px-4 py-2 bg-green-600 hover:bg-green-500 text-white text-sm rounded disabled:opacity-50"
              disabled={acting}
              onClick={() => doAction("start")}
            >
              Start
            </button>
          )}
        </div>
      </div>

      {/* Storage */}
      {service.storage.length > 0 && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Storage
          </h2>
          <div class="space-y-2">
            {service.storage.map((vol) => (
              <StorageRow
                key={vol.name}
                vol={vol}
                serviceId={service.id}
                onUpdated={fetchService}
              />
            ))}
          </div>
        </section>
      )}

      {/* Configuration (install variables) */}
      {service.installed &&
        service.install_variables.length > 0 &&
        (() => {
          const hasCredentials = service.install_variables.some(
            (v) =>
              (v.input_type === "password" || v.input_type === "text") &&
              v.key in service.env_overrides,
          );
          const visibleVars = service.install_variables.filter(
            (v) => v.key in service.env_overrides,
          );
          if (visibleVars.length === 0) return null;
          return (
            <section>
              <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
                Configuration
              </h2>
              <div class="space-y-2">
                {visibleVars.map((v) => (
                  <ConfigRow
                    key={v.key}
                    label={v.label}
                    value={service.env_overrides[v.key]}
                    isPassword={v.input_type === "password"}
                  />
                ))}
                {hasCredentials && id && (
                  <button
                    class="text-xs text-gray-500 hover:text-gray-300 mt-1"
                    onClick={async () => {
                      await api.dismissCredentials(id);
                      fetchService();
                    }}
                  >
                    I've saved these — dismiss credentials
                  </button>
                )}
              </div>
            </section>
          );
        })()}

      {/* Backup */}
      {service.installed && service.backup_supported && id && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Backup
          </h2>
          <div class="bg-gray-900 rounded-lg p-5">
            <BackupForm serviceId={id} />
          </div>
        </section>
      )}

      {/* Logs */}
      {service.installed && id && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Logs
          </h2>
          <LogViewer serviceId={id} />
        </section>
      )}

      {/* Danger Zone */}
      {service.installed && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Danger Zone
          </h2>
          <div class="border border-red-500/30 rounded-lg p-5">
            <div class="flex items-center justify-between">
              <div>
                <h3 class="text-gray-200 font-medium">Remove Service</h3>
                <p class="text-sm text-gray-400 mt-1">
                  Stops containers and removes configuration. Your data files
                  are kept.
                </p>
              </div>
              {!confirmRemove ? (
                <button
                  class="px-4 py-2 bg-red-600/80 hover:bg-red-500 text-white text-sm rounded"
                  onClick={() => setConfirmRemove(true)}
                >
                  Remove
                </button>
              ) : (
                <div class="flex gap-2">
                  <button
                    class="px-4 py-2 bg-red-600 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50"
                    disabled={acting}
                    onClick={handleRemove}
                  >
                    Confirm
                  </button>
                  <button
                    class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
                    onClick={() => setConfirmRemove(false)}
                  >
                    Cancel
                  </button>
                </div>
              )}
            </div>
          </div>
        </section>
      )}
    </div>
  );
}
