import { useState, useCallback } from "preact/hooks";
import { route } from "preact-router";
import { api, linkify, type ServiceInfo } from "../api";
import { usePolling } from "../hooks/use-polling";
import { getServiceStatus, statusColors, statusLabels } from "../components/service-card";
import { LogViewer } from "../components/log-viewer";
import { BackupForm } from "../components/backup-form";
import { StorageRow } from "../components/storage-row";
import { ConfigRow } from "../components/config-row";
import { ServiceBackupActions } from "../components/service-backup-actions";

interface Props {
  id?: string;
}

function ConfigSection({ service, id, onRefresh }: { service: ServiceInfo; id: string; onRefresh: () => void }) {
  const visibleVars = service.install_variables.filter(
    (v) => v.key in service.env_overrides,
  );
  if (visibleVars.length === 0) return null;

  const hasCredentials = visibleVars.some(
    (v) => v.input_type === "password" || v.input_type === "text",
  );

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
        {hasCredentials && (
          <button
            class="text-xs text-gray-500 hover:text-gray-300 mt-1"
            onClick={async () => {
              await api.dismissCredentials(id);
              onRefresh();
            }}
          >
            I've saved these — dismiss credentials
          </button>
        )}
      </div>
    </section>
  );
}

export function ServiceDetail({ id }: Props) {
  const fetcher = useCallback(
    () => api.services().then((all) => all.find((s) => s.id === id) ?? null),
    [id],
  );
  const [service, loading, fetchService] = usePolling<ServiceInfo | null>(fetcher);
  const [acting, setActing] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [editingName, setEditingName] = useState(false);
  const [nameInput, setNameInput] = useState("");

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

  return (
    <div class="flex-1 px-6 py-6 max-w-4xl mx-auto w-full space-y-6">
      {/* Header */}
      <div class="flex items-center justify-between flex-wrap gap-4">
        <div>
          <div class="flex items-center gap-3">
            {editingName ? (
              <input
                type="text"
                value={nameInput}
                onInput={(e) =>
                  setNameInput((e.target as HTMLInputElement).value)
                }
                onKeyDown={async (e) => {
                  if (e.key === "Enter" && id) {
                    await api.renameService(id, nameInput);
                    setEditingName(false);
                    fetchService();
                  } else if (e.key === "Escape") {
                    setEditingName(false);
                  }
                }}
                class="text-2xl font-bold text-gray-100 bg-gray-800 border border-gray-600 rounded px-2 py-0.5 focus:outline-none focus:border-gray-400"
                autoFocus
              />
            ) : (
              <h1
                class="text-2xl font-bold text-gray-100 cursor-pointer hover:text-gray-300"
                onClick={() => {
                  if (service.installed) {
                    setNameInput(service.name);
                    setEditingName(true);
                  }
                }}
                title={service.installed ? "Click to rename" : undefined}
              >
                {service.name}
              </h1>
            )}
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

      {/* Setup Notes */}
      {service.installed && service.post_install_notes && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Setup Notes
          </h2>
          <div class="bg-amber-900/20 border border-amber-500/30 rounded-lg p-5 space-y-2">
            {service.post_install_notes.split("\\n").map((line, i) => (
              <p
                key={i}
                class="text-gray-300 text-sm"
                dangerouslySetInnerHTML={{ __html: linkify(line) }}
              />
            ))}
          </div>
        </section>
      )}

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
      {service.installed && service.install_variables.length > 0 && id && (
        <ConfigSection
          service={service}
          id={id}
          onRefresh={fetchService}
        />
      )}

      {/* Backup */}
      {service.installed && service.backup_supported && id && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Backup
          </h2>
          <div class="bg-gray-900 rounded-lg p-5">
            <BackupForm serviceId={id} backupPassword={service.backup_password} />
          </div>
          <ServiceBackupActions serviceId={id} />
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
