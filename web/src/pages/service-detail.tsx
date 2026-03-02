import { useState, useCallback, useEffect } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  linkify,
  type ServiceInfo,
  type CloudflareStatus,
  type CloudflareZone,
} from "../api";
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
  const [editingHostname, setEditingHostname] = useState(false);
  const [hostnameInput, setHostnameInput] = useState("");
  const [updating, setUpdating] = useState(false);
  const [updateLines, setUpdateLines] = useState<string[]>([]);
  const [cfStatus, setCfStatus] = useState<CloudflareStatus | null>(null);
  const [zones, setZones] = useState<CloudflareZone[] | null>(null);
  const [showDomainForm, setShowDomainForm] = useState(false);
  const [domainSubdomain, setDomainSubdomain] = useState("");
  const [domainZoneId, setDomainZoneId] = useState("");
  const [domainSaving, setDomainSaving] = useState(false);
  const [domainError, setDomainError] = useState("");

  useEffect(() => {
    api.cloudflareStatus().then(setCfStatus).catch(() => {});
  }, []);

  const loadZones = async () => {
    if (zones) return;
    try {
      const z = await api.cloudflareZones();
      setZones(z);
      if (z.length > 0 && !domainZoneId) setDomainZoneId(z[0].id);
    } catch {}
  };

  const handleBindDomain = async () => {
    if (!id || !domainZoneId) return;
    const zone = zones?.find((z) => z.id === domainZoneId);
    if (!zone) return;
    setDomainSaving(true);
    setDomainError("");
    try {
      await api.bindDomain(id, {
        subdomain: domainSubdomain.trim(),
        zone_id: zone.id,
        zone_name: zone.name,
      });
      setShowDomainForm(false);
      setDomainSubdomain("");
      fetchService();
    } catch (e: unknown) {
      setDomainError(e instanceof Error ? e.message : "Failed to bind domain");
    } finally {
      setDomainSaving(false);
    }
  };

  const handleUnbindDomain = async () => {
    if (!id) return;
    setDomainSaving(true);
    try {
      await api.unbindDomain(id);
      fetchService();
    } catch (e: unknown) {
      setDomainError(e instanceof Error ? e.message : "Failed to unbind");
    } finally {
      setDomainSaving(false);
    }
  };

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

  const handleUpdate = () => {
    if (!id) return;
    setUpdating(true);
    setUpdateLines([]);
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${proto}//${window.location.host}/api/services/${id}/update`);
    ws.onmessage = (e) => {
      const msg = e.data;
      if (msg === "__DONE__") {
        ws.close();
        setUpdating(false);
        fetchService();
      } else {
        setUpdateLines((prev) => [...prev, msg]);
      }
    };
    ws.onerror = () => {
      setUpdating(false);
    };
    ws.onclose = () => {
      setUpdating(false);
    };
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
          {status === "running" && service.domain_url && (
            <button
              class="px-4 py-2 bg-orange-600 hover:bg-orange-500 text-white text-sm rounded"
              onClick={() => window.open(service.domain_url!, "_blank")}
            >
              Domain
            </button>
          )}
          {status === "running" && service.tailscale_url && (
            <button
              class="px-4 py-2 bg-purple-600 hover:bg-purple-500 text-white text-sm rounded"
              onClick={() => window.open(service.tailscale_url!, "_blank")}
            >
              Tailscale
            </button>
          )}
          {status === "running" && service.port && (
            <button
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
              onClick={() =>
                window.open(
                  `http://${window.location.hostname}:${service.port}${service.web_path || ""}`,
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

      {/* Update banner */}
      {service.installed && service.update_available && (
        <section class="bg-blue-900/20 border border-blue-500/30 rounded-lg p-4">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-blue-300">Update Available</h3>
              <p class="text-xs text-gray-400 mt-0.5">
                A newer Docker image is available for this service.
              </p>
            </div>
            <button
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
              disabled={updating}
              onClick={handleUpdate}
            >
              {updating ? "Updating..." : "Update"}
            </button>
          </div>
          {updateLines.length > 0 && (
            <pre class="mt-3 bg-gray-950 rounded p-3 text-xs text-gray-300 max-h-48 overflow-y-auto font-mono">
              {updateLines.join("\n")}
            </pre>
          )}
        </section>
      )}

      {/* Tailscale toggle */}
      {service.installed && service.tailscale_url !== undefined && id && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-gray-300">Tailscale Access</h3>
              <p class="text-xs text-gray-500 mt-0.5">
                {service.tailscale_disabled
                  ? "Sidecar disabled for this service"
                  : service.tailscale_url
                    ? service.tailscale_url
                    : "Tailnet not detected yet"}
              </p>
            </div>
            <button
              class={`px-3 py-1.5 text-xs rounded ${
                service.tailscale_disabled
                  ? "bg-green-600/80 hover:bg-green-500 text-white"
                  : "bg-gray-600 hover:bg-gray-500 text-gray-200"
              }`}
              onClick={async () => {
                await api.toggleServiceTailscale(id, !service.tailscale_disabled);
                fetchService();
              }}
            >
              {service.tailscale_disabled ? "Enable" : "Disable"}
            </button>
          </div>
          {!service.tailscale_disabled && (
            <div class="flex items-center gap-2 pt-1 border-t border-gray-800">
              <span class="text-xs text-gray-500 shrink-0">Hostname:</span>
              {editingHostname ? (
                <input
                  type="text"
                  value={hostnameInput}
                  onInput={(e) =>
                    setHostnameInput((e.target as HTMLInputElement).value)
                  }
                  onKeyDown={async (e) => {
                    if (e.key === "Enter" && id) {
                      await api.toggleServiceTailscale(
                        id,
                        false,
                        hostnameInput.trim(),
                      );
                      setEditingHostname(false);
                      fetchService();
                    } else if (e.key === "Escape") {
                      setEditingHostname(false);
                    }
                  }}
                  class="text-xs text-gray-300 bg-gray-800 border border-gray-600 rounded px-2 py-0.5 focus:outline-none focus:border-gray-400 flex-1"
                  placeholder={`myground-${id}`}
                  autoFocus
                />
              ) : (
                <span
                  class="text-xs text-gray-300 cursor-pointer hover:text-gray-100"
                  onClick={() => {
                    setHostnameInput(
                      service.tailscale_hostname || `myground-${id}`,
                    );
                    setEditingHostname(true);
                  }}
                  title="Click to edit hostname"
                >
                  {service.tailscale_hostname || `myground-${id}`}
                </span>
              )}
            </div>
          )}
        </section>
      )}

      {/* LAN Access toggle */}
      {service.installed && id && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-gray-300">LAN Access</h3>
              <p class="text-xs text-gray-500 mt-0.5">
                {service.lan_accessible
                  ? "Binding to 0.0.0.0 — accessible from your local network"
                  : "Binding to 127.0.0.1 — localhost only"}
              </p>
            </div>
            <button
              class={`px-3 py-1.5 text-xs rounded ${
                service.lan_accessible
                  ? "bg-gray-600 hover:bg-gray-500 text-gray-200"
                  : "bg-green-600/80 hover:bg-green-500 text-white"
              }`}
              onClick={async () => {
                await api.toggleServiceLan(id, !service.lan_accessible);
                fetchService();
              }}
            >
              {service.lan_accessible ? "Disable" : "Enable"}
            </button>
          </div>
        </section>
      )}

      {/* Domain (Cloudflare) */}
      {service.installed && id && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <h3 class="text-sm font-medium text-gray-300">Custom Domain</h3>
          {!cfStatus?.enabled ? (
            <p class="text-xs text-gray-500">
              Enable Cloudflare in settings to expose this service on a custom
              domain.
            </p>
          ) : service.domain_url ? (
            <div class="flex items-center justify-between">
              <a
                href={service.domain_url}
                target="_blank"
                rel="noopener noreferrer"
                class="text-amber-400 hover:text-amber-300 text-sm font-mono underline"
              >
                {service.domain_url}
              </a>
              <button
                onClick={handleUnbindDomain}
                disabled={domainSaving}
                class="px-3 py-1.5 bg-gray-600 hover:bg-gray-500 text-gray-200 text-xs rounded disabled:opacity-50"
              >
                {domainSaving ? "..." : "Remove"}
              </button>
            </div>
          ) : !showDomainForm ? (
            <button
              onClick={() => {
                setShowDomainForm(true);
                loadZones();
              }}
              class="px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white text-xs rounded"
            >
              Add Domain
            </button>
          ) : (
            <div class="space-y-2">
              <div class="flex gap-2">
                <input
                  type="text"
                  value={domainSubdomain}
                  onInput={(e) =>
                    setDomainSubdomain(
                      (e.target as HTMLInputElement).value,
                    )
                  }
                  placeholder="subdomain (or empty for apex)"
                  class="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                />
                <select
                  value={domainZoneId}
                  onChange={(e) =>
                    setDomainZoneId(
                      (e.target as HTMLSelectElement).value,
                    )
                  }
                  class="px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                >
                  {zones ? (
                    zones.map((z) => (
                      <option key={z.id} value={z.id}>
                        {z.name}
                      </option>
                    ))
                  ) : (
                    <option>Loading...</option>
                  )}
                </select>
              </div>
              <div class="flex gap-2">
                <button
                  onClick={handleBindDomain}
                  disabled={domainSaving || !domainZoneId}
                  class="px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white text-xs rounded disabled:opacity-50"
                >
                  {domainSaving ? "Binding..." : "Bind"}
                </button>
                <button
                  onClick={() => setShowDomainForm(false)}
                  class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                >
                  Cancel
                </button>
              </div>
              {domainError && (
                <p class="text-red-400 text-xs">{domainError}</p>
              )}
            </div>
          )}
        </section>
      )}

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
