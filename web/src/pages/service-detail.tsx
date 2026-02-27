import { useState, useEffect, useCallback } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatBytes,
  type ServiceInfo,
  type DiskInfo,
  type StorageVolumeStatus,
} from "../api";
import { getServiceStatus } from "../components/service-card";
import { LogViewer } from "../components/log-viewer";
import { BackupForm } from "../components/backup-form";

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
  const [disks, setDisks] = useState<DiskInfo[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (editing) {
      api.disks().then(setDisks).catch(() => setDisks([]));
    }
  }, [editing]);

  const handleChange = async (mountPoint: string) => {
    setSaving(true);
    try {
      const newPath = `${mountPoint}/myground/${serviceId}/${vol.name}/`;
      await api.updateStorage(serviceId, { [vol.name]: newPath });
      onUpdated();
      setEditing(false);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="bg-gray-900 rounded-lg px-4 py-3">
      <div class="flex items-center justify-between">
        <div>
          <span class="text-gray-200 capitalize">{vol.name}</span>
          {vol.host_path && (
            <span class="text-xs text-gray-500 ml-2 font-mono">
              {vol.host_path}
            </span>
          )}
        </div>
        <div class="flex items-center gap-3">
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
          {disks.map((disk) => (
            <button
              key={disk.mount_point}
              class="w-full bg-gray-800 hover:bg-gray-700 rounded p-3 text-left text-sm flex items-center justify-between disabled:opacity-50"
              disabled={saving}
              onClick={() => handleChange(disk.mount_point)}
            >
              <span class="text-gray-200">{disk.mount_point}</span>
              <span class="text-gray-400">
                {formatBytes(disk.available_bytes)} free
              </span>
            </button>
          ))}
        </div>
      )}
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

      {/* Backup */}
      {service.installed && id && (
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
      {status === "running" && id && (
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
                    Yes, remove
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
