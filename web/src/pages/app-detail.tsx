import { useState, useCallback, useEffect } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  linkify,
  shortDigest,
  type AppInfo,
  type CloudflareStatus,
  type CloudflareZone,
  type VpnConfig,
  type HealthResponse,
} from "../api";
import { usePolling } from "../hooks/use-polling";
import { getAppStatus, statusColors, statusLabels } from "../components/app-card";
import { LogViewer } from "../components/log-viewer";
import { StorageRow } from "../components/storage-row";
import { ConfigRow } from "../components/config-row";
import { AppBackupJobs } from "../components/app-backup-jobs";
import { InstallModal } from "../components/install-modal";

interface Props {
  id?: string;
}

function ConfigSection({ app, id, onRefresh }: { app: AppInfo; id: string; onRefresh: () => void }) {
  const visibleVars = app.install_variables.filter(
    (v) => v.key in app.env_overrides,
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
            value={app.env_overrides[v.key]}
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

export function AppDetail({ id }: Props) {
  const fetcher = useCallback(
    () => api.apps().then((all) => all.find((s) => s.id === id) ?? null),
    [id],
  );
  const [app, loading, fetchApp] = usePolling<AppInfo | null>(fetcher);
  const [acting, setActing] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [removeStatus, setRemoveStatus] = useState("");
  const [editingName, setEditingName] = useState(false);
  const [nameInput, setNameInput] = useState("");
  const [editingHostname, setEditingHostname] = useState(false);
  const [hostnameInput, setHostnameInput] = useState("");
  const [savingHostname, setSavingHostname] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateLines, setUpdateLines] = useState<string[]>([]);
  const [cfStatus, setCfStatus] = useState<CloudflareStatus | null>(null);
  const [zones, setZones] = useState<CloudflareZone[] | null>(null);
  const [showDomainForm, setShowDomainForm] = useState(false);
  const [domainSubdomain, setDomainSubdomain] = useState("");
  const [domainZoneId, setDomainZoneId] = useState("");
  const [domainSaving, setDomainSaving] = useState(false);
  const [domainError, setDomainError] = useState("");
  const [dismissedUpdate, setDismissedUpdate] = useState(false);
  const [showInstallModal, setShowInstallModal] = useState(false);
  const [showVpnForm, setShowVpnForm] = useState(false);
  const [vpnProvider, setVpnProvider] = useState("protonvpn");
  const [vpnType, setVpnType] = useState("openvpn");
  const [vpnCountry, setVpnCountry] = useState("");
  const [vpnPortForward, setVpnPortForward] = useState(true);
  const [vpnEnvVars, setVpnEnvVars] = useState<Record<string, string>>({});
  const [vpnSaving, setVpnSaving] = useState(false);
  const [vpnError, setVpnError] = useState("");
  const [globalVpn, setGlobalVpn] = useState<VpnConfig | null>(null);
  const [tsSaving, setTsSaving] = useState(false);
  const [lanSaving, setLanSaving] = useState(false);
  const [healthData, setHealthData] = useState<HealthResponse | null>(null);
  const serverIp = healthData?.server_ip;
  const availableGpus = healthData?.available_gpus ?? [];

  useEffect(() => {
    api.cloudflareStatus().then(setCfStatus).catch(() => {});
    api.getVpnConfig().then(setGlobalVpn).catch(() => {});
    api.health().then(setHealthData).catch(() => {});
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
      fetchApp();
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
      fetchApp();
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
      if (action === "start") await api.startApp(id);
      else await api.stopApp(id);
      setTimeout(fetchApp, 1000);
    } finally {
      setActing(false);
    }
  };

  const handleUpdate = () => {
    if (!id) return;
    setUpdating(true);
    setUpdateLines([]);
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(`${proto}//${window.location.host}/api/apps/${id}/update`);
    ws.onmessage = (e) => {
      const msg = e.data;
      if (msg === "__DONE__") {
        ws.close();
        setUpdating(false);
        fetchApp();
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
    setRemoving(true);
    setRemoveStatus("Stopping containers...");
    try {
      await api.stopApp(id).catch(() => {});
      setRemoveStatus("Removing containers and volumes...");
      await api.removeApp(id);
      setRemoveStatus("Done!");
      setTimeout(() => route("/"), 500);
    } catch (e) {
      setRemoveStatus(`Error: ${e instanceof Error ? e.message : "Remove failed"}`);
      setTimeout(() => {
        setRemoving(false);
        setRemoveStatus("");
      }, 3000);
    }
  };

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  if (!app) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">App not found.</p>
      </div>
    );
  }

  const status = getAppStatus(app);

  return (
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full space-y-4 sm:space-y-6">
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
                    await api.renameApp(id, nameInput);
                    setEditingName(false);
                    fetchApp();
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
                  if (app.installed) {
                    setNameInput(app.name);
                    setEditingName(true);
                  }
                }}
                title={app.installed ? "Click to rename" : undefined}
              >
                {app.name}
              </h1>
            )}
            <span class={`text-sm font-medium ${statusColors[status]}`}>
              {statusLabels[status]}
            </span>
          </div>
          <p class="text-gray-400 mt-1">{app.description}</p>
        </div>
        <div class="flex gap-2">
          {!app.installed && (
            <button
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
              onClick={() => setShowInstallModal(true)}
            >
              Install
            </button>
          )}
          {(status === "running" || status === "health_checking") && app.domain_url && (
            <button
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
              onClick={() => window.open(app.domain_url!, "_blank")}
            >
              {status === "health_checking" ? "Try Open" : "Open"}
            </button>
          )}
          {(status === "running" || status === "health_checking") && app.tailscale_url && !app.tailscale_disabled && (
            <button
              class="px-4 py-2 bg-purple-600 hover:bg-purple-500 text-white text-sm rounded"
              onClick={() => window.open(app.tailscale_url!, "_blank")}
            >
              {status === "health_checking" ? "Try Tailnet" : "Open via Tailnet"}
            </button>
          )}
          {(status === "running" || status === "health_checking") && app.lan_accessible && app.port && serverIp && (
            <button
              class="px-4 py-2 bg-teal-600 hover:bg-teal-500 text-white text-sm rounded"
              onClick={() =>
                window.open(
                  `http://${serverIp}:${app.port}${app.web_path || ""}`,
                  "_blank",
                )
              }
            >
              {status === "health_checking" ? "Try LAN" : "Open via LAN"}
            </button>
          )}
          {(status === "running" || status === "health_checking") && (
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
      {app.installed && app.update_available && !dismissedUpdate && (
        <section class="bg-blue-900/20 border border-blue-500/30 rounded-lg p-4">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-blue-300">Update Available</h3>
              <p class="text-xs text-gray-400 mt-0.5">
                {app.current_digest && app.latest_digest ? (
                  <span class="inline-flex items-center gap-1">
                    <span class="font-mono text-gray-400">{shortDigest(app.current_digest)}</span>
                    <svg class="w-3 h-3 text-gray-400" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path d="M13.5 4.5 21 12m0 0-7.5 7.5M21 12H3" /></svg>
                    <span class="font-mono text-blue-400">{shortDigest(app.latest_digest)}</span>
                  </span>
                ) : (
                  "A newer Docker image is available for this app."
                )}
              </p>
            </div>
            <div class="flex items-center gap-2">
              <button
                class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
                disabled={updating}
                onClick={handleUpdate}
              >
                {updating ? "Updating..." : "Update"}
              </button>
              <button
                class="p-1 text-gray-400 hover:text-gray-200"
                onClick={() => setDismissedUpdate(true)}
                title="Dismiss"
              >
                <svg class="w-4 h-4" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                  <path d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          </div>
          {updateLines.length > 0 && (
            <pre class="mt-3 bg-gray-950 rounded p-3 text-xs text-gray-300 max-h-48 overflow-y-auto font-mono">
              {updateLines.join("\n")}
            </pre>
          )}
        </section>
      )}

      {/* Tailscale toggle */}
      {app.installed && app.supports_tailscale && id && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-gray-300">Tailscale Access</h3>
              <p class="text-xs text-gray-500 mt-0.5">
                {app.tailscale_disabled
                  ? "Sidecar disabled for this app"
                  : app.tailscale_url
                    ? <span>Available at{" "}<a href={app.tailscale_url} target="_blank" rel="noopener noreferrer" class="text-gray-300 hover:text-gray-100 underline">{app.tailscale_url}</a></span>
                    : "Tailnet not detected yet"}
              </p>
            </div>
            <button
              class={`px-3 py-1.5 text-xs rounded disabled:opacity-50 ${
                app.tailscale_disabled
                  ? "bg-green-600/80 hover:bg-green-500 text-white"
                  : "bg-gray-600 hover:bg-gray-500 text-gray-200"
              }`}
              disabled={tsSaving}
              onClick={async () => {
                setTsSaving(true);
                try {
                  await api.toggleAppTailscale(id, !app.tailscale_disabled);
                  fetchApp();
                } finally {
                  setTsSaving(false);
                }
              }}
            >
              {tsSaving
                ? app.tailscale_disabled ? "Adding Tailscale sidecar & restarting..." : "Removing Tailscale sidecar & restarting..."
                : app.tailscale_disabled ? "Enable" : "Disable"}
            </button>
          </div>
          {!app.tailscale_disabled && (
            <div class="flex items-center gap-2 pt-2 border-t border-gray-800">
              {editingHostname ? (
                <>
                  <span class="text-xs text-gray-500 shrink-0">Hostname:</span>
                  <input
                    type="text"
                    value={hostnameInput}
                    disabled={savingHostname}
                    onInput={(e) =>
                      setHostnameInput((e.target as HTMLInputElement).value)
                    }
                    onKeyDown={async (e) => {
                      if (e.key === "Enter" && id && !savingHostname) {
                        setSavingHostname(true);
                        try {
                          await api.toggleAppTailscale(
                            id,
                            false,
                            hostnameInput.trim(),
                          );
                          setEditingHostname(false);
                          fetchApp();
                        } finally {
                          setSavingHostname(false);
                        }
                      } else if (e.key === "Escape" && !savingHostname) {
                        setEditingHostname(false);
                      }
                    }}
                    class="text-xs text-gray-300 bg-gray-800 border border-gray-700 rounded px-2 py-1 focus:outline-none focus:border-gray-500 flex-1 disabled:opacity-50"
                    placeholder={`myground-${id}`}
                    autoFocus
                  />
                  <button
                    class="px-3 py-1 text-xs rounded bg-gray-600 hover:bg-gray-500 text-gray-200 disabled:opacity-50"
                    disabled={savingHostname}
                    onClick={async () => {
                      setSavingHostname(true);
                      try {
                        await api.toggleAppTailscale(id, false, hostnameInput.trim());
                        setEditingHostname(false);
                        fetchApp();
                      } finally {
                        setSavingHostname(false);
                      }
                    }}
                  >
                    {savingHostname ? "Saving..." : "Save"}
                  </button>
                  <button
                    class="px-3 py-1 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-400 disabled:opacity-50"
                    disabled={savingHostname}
                    onClick={() => setEditingHostname(false)}
                  >
                    Cancel
                  </button>
                </>
              ) : (
                <>
                  <span class="text-xs text-gray-500">
                    Hostname: <span class="text-gray-300">{app.tailscale_hostname || `myground-${id}`}</span>
                  </span>
                  <button
                    class="text-xs text-gray-500 hover:text-gray-300"
                    onClick={() => {
                      setHostnameInput(
                        app.tailscale_hostname || `myground-${id}`,
                      );
                      setEditingHostname(true);
                    }}
                  >
                    Rename
                  </button>
                </>
              )}
            </div>
          )}
        </section>
      )}

      {/* LAN Access toggle */}
      {app.installed && id && app.port && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-gray-300">LAN Access</h3>
              <p class="text-xs text-gray-500 mt-0.5">
                {app.lan_accessible
                  ? "Binding to 0.0.0.0 — accessible from your local network"
                  : "Binding to 127.0.0.1 — localhost only"}
              </p>
            </div>
            <button
              class={`px-3 py-1.5 text-xs rounded disabled:opacity-50 ${
                app.lan_accessible
                  ? "bg-gray-600 hover:bg-gray-500 text-gray-200"
                  : "bg-green-600/80 hover:bg-green-500 text-white"
              }`}
              disabled={lanSaving}
              onClick={async () => {
                setLanSaving(true);
                try {
                  await api.toggleAppLan(id, !app.lan_accessible);
                  fetchApp();
                } finally {
                  setLanSaving(false);
                }
              }}
            >
              {lanSaving
                ? app.lan_accessible ? "Rebinding to localhost & restarting..." : "Rebinding to 0.0.0.0 & restarting..."
                : app.lan_accessible ? "Disable" : "Enable"}
            </button>
          </div>
        </section>
      )}

      {/* GPU Acceleration */}
      {app.installed && app.supports_gpu && id && availableGpus.length > 0 && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-gray-300">GPU Acceleration</h3>
              <p class="text-xs text-gray-500 mt-0.5">
                {app.gpu_mode === "nvidia"
                  ? "NVIDIA GPU passthrough enabled"
                  : app.gpu_mode === "intel"
                    ? "Intel/AMD iGPU passthrough enabled (/dev/dri)"
                    : "No GPU acceleration"}
              </p>
            </div>
            <div class="flex gap-1.5">
              {(["none", ...availableGpus] as string[]).map((mode) => (
                <button
                  key={mode}
                  class={`px-3 py-1.5 text-xs rounded ${
                    (app.gpu_mode ?? "none") === mode
                      ? "bg-amber-600 text-white"
                      : "bg-gray-700 hover:bg-gray-600 text-gray-300"
                  }`}
                  onClick={async () => {
                    await api.setAppGpu(id, mode);
                    fetchApp();
                  }}
                >
                  {mode === "none" ? "None" : mode === "nvidia" ? "NVIDIA" : "Intel/AMD"}
                </button>
              ))}
            </div>
          </div>
        </section>
      )}

      {/* VPN Sidecar */}
      {app.installed && id && !app.uses_host_network && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="flex items-center justify-between">
            <div>
              <h3 class="text-sm font-medium text-gray-300">VPN</h3>
              <p class="text-xs text-gray-500 mt-0.5">
                {app.vpn_enabled
                  ? `Traffic routed through ${app.vpn_provider || "VPN"}`
                  : globalVpn?.provider
                    ? `Using ${globalVpn.provider} (global config)`
                    : "Route this app's traffic through a VPN"}
              </p>
            </div>
            {app.vpn_enabled ? (
              <button
                class="px-3 py-1.5 text-xs rounded bg-gray-600 hover:bg-gray-500 text-gray-200 disabled:opacity-50"
                disabled={vpnSaving}
                onClick={async () => {
                  setVpnSaving(true);
                  setVpnError("");
                  try {
                    await api.setAppVpn(id, { enabled: false });
                    setShowVpnForm(false);
                    fetchApp();
                  } catch (e: unknown) {
                    setVpnError(e instanceof Error ? e.message : "Failed");
                  } finally {
                    setVpnSaving(false);
                  }
                }}
              >
                {vpnSaving ? "Removing VPN sidecar & restarting..." : "Disable"}
              </button>
            ) : globalVpn?.provider ? (
              <button
                class="px-3 py-1.5 text-xs rounded bg-green-600/80 hover:bg-green-500 text-white disabled:opacity-50"
                disabled={vpnSaving}
                onClick={async () => {
                  setVpnSaving(true);
                  setVpnError("");
                  try {
                    await api.setAppVpn(id, { enabled: true });
                    fetchApp();
                  } catch (e: unknown) {
                    setVpnError(e instanceof Error ? e.message : "Failed to enable VPN");
                  } finally {
                    setVpnSaving(false);
                  }
                }}
              >
                {vpnSaving ? "Injecting VPN sidecar & restarting..." : "Enable"}
              </button>
            ) : !showVpnForm ? (
              <button
                class="px-3 py-1.5 text-xs rounded bg-green-600/80 hover:bg-green-500 text-white"
                onClick={() => setShowVpnForm(true)}
              >
                Enable
              </button>
            ) : null}
          </div>
          {vpnError && <p class="text-red-400 text-xs">{vpnError}</p>}
          {showVpnForm && !app.vpn_enabled && !globalVpn?.provider && (
            <div class="space-y-3 pt-2 border-t border-gray-800">
              <div>
                <label class="block text-xs text-gray-400 mb-1">Provider</label>
                <select
                  value={vpnProvider}
                  onChange={(e) => {
                    setVpnProvider((e.target as HTMLSelectElement).value);
                    setVpnEnvVars({});
                  }}
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                >
                  <option value="protonvpn">ProtonVPN</option>
                  <option value="nordvpn">NordVPN</option>
                  <option value="mullvad">Mullvad</option>
                  <option value="custom">Custom</option>
                </select>
              </div>
              <div>
                <label class="block text-xs text-gray-400 mb-1">VPN Type</label>
                <select
                  value={vpnType}
                  onChange={(e) => setVpnType((e.target as HTMLSelectElement).value)}
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                >
                  <option value="openvpn">OpenVPN</option>
                  <option value="wireguard">WireGuard</option>
                </select>
              </div>
              {vpnType === "openvpn" && (
                <>
                  <div>
                    <label class="block text-xs text-gray-400 mb-1">Username</label>
                    <input
                      type="text"
                      value={vpnEnvVars["OPENVPN_USER"] || ""}
                      onInput={(e) => setVpnEnvVars({ ...vpnEnvVars, OPENVPN_USER: (e.target as HTMLInputElement).value })}
                      class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                    />
                  </div>
                  <div>
                    <label class="block text-xs text-gray-400 mb-1">Password</label>
                    <input
                      type="password"
                      value={vpnEnvVars["OPENVPN_PASSWORD"] || ""}
                      onInput={(e) => setVpnEnvVars({ ...vpnEnvVars, OPENVPN_PASSWORD: (e.target as HTMLInputElement).value })}
                      class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                    />
                  </div>
                  {vpnProvider === "protonvpn" && (
                    <p class="text-xs text-gray-500">
                      Use your <a href="https://account.protonvpn.com/account#openvpn" target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline">OpenVPN/IKEv2 credentials</a>, not your Proton account password. Required if you have 2FA enabled.
                      {vpnPortForward && " Append +pmp to your username (e.g. user123+pmp) for port forwarding to work."}
                    </p>
                  )}
                </>
              )}
              {vpnType === "wireguard" && (
                <div>
                  <label class="block text-xs text-gray-400 mb-1">Private Key</label>
                  <input
                    type="password"
                    value={vpnEnvVars["WIREGUARD_PRIVATE_KEY"] || ""}
                    onInput={(e) => setVpnEnvVars({ ...vpnEnvVars, WIREGUARD_PRIVATE_KEY: (e.target as HTMLInputElement).value })}
                    class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                  />
                </div>
              )}
              <div>
                <label class="block text-xs text-gray-400 mb-1">
                  Server Country (optional) — <a href={{ protonvpn: "https://protonvpn.com/vpn-servers", nordvpn: "https://nordvpn.com/servers/", mullvad: "https://mullvad.net/en/servers", custom: "https://github.com/qdm12/gluetun-wiki/tree/main/setup/providers" }[vpnProvider] || "https://github.com/qdm12/gluetun-wiki/tree/main/setup/providers"} target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline">see supported countries</a>
                </label>
                <input
                  type="text"
                  value={vpnCountry}
                  onInput={(e) => setVpnCountry((e.target as HTMLInputElement).value)}
                  placeholder="e.g. Netherlands"
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                />
              </div>
              <div>
                <label class="flex items-center gap-2 text-sm text-gray-300">
                  <input
                    type="checkbox"
                    checked={vpnPortForward}
                    onChange={(e) => setVpnPortForward((e.target as HTMLInputElement).checked)}
                    class="rounded bg-gray-800 border-gray-600"
                  />
                  Enable port forwarding (recommended)
                </label>
                <p class="text-xs text-gray-500 mt-1">
                  Required for torrent seeding and other apps that need to accept incoming connections.
                  Leave this on unless you know you don't need it.{id?.startsWith("qbittorrent") ? " Essential for qBittorrent seeding." : ""}
                </p>
              </div>
              <div class="flex gap-2">
                <button
                  disabled={vpnSaving}
                  class="px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white text-xs rounded disabled:opacity-50"
                  onClick={async () => {
                    setVpnSaving(true);
                    setVpnError("");
                    try {
                      const config: VpnConfig = {
                        enabled: true,
                        provider: vpnProvider,
                        vpn_type: vpnType,
                        server_countries: vpnCountry || undefined,
                        port_forwarding: vpnPortForward,
                        env_vars: vpnEnvVars,
                      };
                      await api.setAppVpn(id, config);
                      setShowVpnForm(false);
                      fetchApp();
                    } catch (e: unknown) {
                      setVpnError(e instanceof Error ? e.message : "Failed to enable VPN");
                    } finally {
                      setVpnSaving(false);
                    }
                  }}
                >
                  {vpnSaving ? "Saving..." : "Save"}
                </button>
                <button
                  class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                  onClick={() => {
                    setShowVpnForm(false);
                    setVpnError("");
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </section>
      )}

      {/* Domain (Cloudflare) */}
      {app.installed && id && (
        <section class="bg-gray-900 rounded-lg p-4 space-y-3">
          <h3 class="text-sm font-medium text-gray-300">Custom Domain</h3>
          {!cfStatus?.enabled ? (
            <p class="text-xs text-gray-500">
              Enable Cloudflare in settings to expose this app on a custom
              domain.
            </p>
          ) : app.domain_url ? (
            <div class="flex items-center justify-between gap-2">
              <a
                href={app.domain_url}
                target="_blank"
                rel="noopener noreferrer"
                class="text-amber-400 hover:text-amber-300 text-sm font-mono underline truncate min-w-0"
              >
                {app.domain_url}
              </a>
              <button
                onClick={handleUnbindDomain}
                disabled={domainSaving}
                class="px-3 py-1.5 bg-gray-600 hover:bg-gray-500 text-gray-200 text-xs rounded disabled:opacity-50 shrink-0"
              >
                {domainSaving ? "Removing..." : "Remove"}
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
      {app.installed && app.post_install_notes && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Setup Notes
          </h2>
          <div class="bg-amber-900/20 border border-amber-500/30 rounded-lg p-5 space-y-2">
            {app.post_install_notes.split("\\n").map((line, i) => (
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
      {app.storage.length > 0 && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Storage
          </h2>
          <div class="space-y-2">
            {app.storage.map((vol) => (
              <StorageRow
                key={vol.name}
                vol={vol}
                appId={app.id}
                onUpdated={fetchApp}
              />
            ))}
          </div>
        </section>
      )}

      {/* Configuration (install variables) */}
      {app.installed && app.install_variables.length > 0 && id && (
        <ConfigSection
          app={app}
          id={id}
          onRefresh={fetchApp}
        />
      )}

      {/* Backup */}
      {app.installed && app.backup_supported && id && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Backup
          </h2>
          <div class="bg-gray-900 rounded-lg p-5">
            <AppBackupJobs appId={id} appName={app.name} hasBackupPassword={app.has_backup_password} storage={app.storage} />
          </div>
        </section>
      )}

      {/* Logs */}
      {app.installed && id && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Logs
          </h2>
          <LogViewer appId={id} />
        </section>
      )}

      {/* Danger Zone */}
      {app.installed && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
            Danger Zone
          </h2>
          <div class="border border-red-500/30 rounded-lg p-5">
            {removing ? (
              <div class="flex items-center gap-3">
                {!removeStatus.startsWith("Error") && !removeStatus.startsWith("Done") && (
                  <svg class="animate-spin h-5 w-5 text-red-400 shrink-0" viewBox="0 0 24 24" fill="none">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                  </svg>
                )}
                <span class={`text-sm ${removeStatus.startsWith("Error") ? "text-red-400" : "text-gray-300"}`}>
                  {removeStatus}
                </span>
              </div>
            ) : (
              <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                <div>
                  <h3 class="text-gray-200 font-medium">Remove App</h3>
                  <p class="text-sm text-gray-400 mt-1">
                    Stops containers and removes configuration. Your data files
                    are kept.
                  </p>
                </div>
                {!confirmRemove ? (
                  <button
                    class="px-4 py-2 bg-red-600/80 hover:bg-red-500 text-white text-sm rounded shrink-0 self-start sm:self-auto"
                    onClick={() => setConfirmRemove(true)}
                  >
                    Remove
                  </button>
                ) : (
                  <div class="flex gap-2 shrink-0">
                    <button
                      class="px-4 py-2 bg-red-600 hover:bg-red-500 text-white text-sm rounded"
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
            )}
          </div>
        </section>
      )}

      {/* Install modal */}
      {showInstallModal && !app.installed && (
        <InstallModal
          appId={app.id}
          appName={app.name}
          hasStorage={!!app.has_storage}
          backupSupported={app.backup_supported}
          installVariables={app.install_variables}
          storageVolumes={app.storage_volumes}
          onClose={() => setShowInstallModal(false)}
          onInstalled={() => {
            setShowInstallModal(false);
            fetchApp();
          }}
        />
      )}
    </div>
  );
}
