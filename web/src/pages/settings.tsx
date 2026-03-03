import { useState, useEffect } from "preact/hooks";
import {
  api,
  formatTimestamp,
  type GlobalConfig,
  type ApiKeyInfo,
  type VpnConfig,
  type AwsSetupResult,
} from "../api";
import { PathPicker } from "../components/path-picker";
import { Field } from "../components/field";
import { AwsSetupForm } from "../components/aws-setup-form";

interface Props {
  onLogout?: () => void;
}

export function Settings({ onLogout }: Props) {
  const [config, setConfig] = useState<GlobalConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingPath, setEditingPath] = useState(false);

  // API Keys state
  const [apiKeys, setApiKeys] = useState<ApiKeyInfo[]>([]);
  const [newKeyName, setNewKeyName] = useState("");
  const [creatingKey, setCreatingKey] = useState(false);
  const [newRawKey, setNewRawKey] = useState<string | null>(null);
  const [keyCopied, setKeyCopied] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  // VPN state
  const [vpnConfig, setVpnConfig] = useState<VpnConfig | null>(null);
  const [vpnProvider, setVpnProvider] = useState("protonvpn");
  const [vpnType, setVpnType] = useState("openvpn");
  const [vpnCountry, setVpnCountry] = useState("");
  const [vpnPortForward, setVpnPortForward] = useState(true);
  const [vpnEnvVars, setVpnEnvVars] = useState<Record<string, string>>({});
  const [vpnSaving, setVpnSaving] = useState(false);
  const [vpnSaved, setVpnSaved] = useState(false);
  const [vpnError, setVpnError] = useState<string | null>(null);

  useEffect(() => {
    api.globalConfig().then(setConfig).catch(() => {});
    api.listApiKeys().then(setApiKeys).catch(() => {});
    api.getVpnConfig().then((cfg) => {
      setVpnConfig(cfg);
      if (cfg.provider) setVpnProvider(cfg.provider);
      if (cfg.vpn_type) setVpnType(cfg.vpn_type);
      if (cfg.server_countries) setVpnCountry(cfg.server_countries);
      setVpnPortForward(cfg.port_forwarding ?? true);
      if (cfg.env_vars) setVpnEnvVars(cfg.env_vars);
    }).catch(() => {});
  }, []);

  const save = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      await api.saveGlobalConfig(config);
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const updateBackup = (key: string, value: string) => {
    if (!config) return;
    setConfig({
      ...config,
      backup: { ...config.backup, [key]: value || undefined },
    });
  };

  const handleCreateKey = async () => {
    if (!newKeyName.trim()) return;
    setCreatingKey(true);
    setKeyError(null);
    setNewRawKey(null);
    try {
      const resp = await api.createApiKey(newKeyName.trim());
      setNewRawKey(resp.key);
      setNewKeyName("");
      const keys = await api.listApiKeys();
      setApiKeys(keys);
    } catch (e: unknown) {
      setKeyError(e instanceof Error ? e.message : "Failed to create key");
    } finally {
      setCreatingKey(false);
    }
  };

  const handleRevokeKey = async (id: string) => {
    try {
      await api.revokeApiKey(id);
      setApiKeys((prev) => prev.filter((k) => k.id !== id));
    } catch (e: unknown) {
      setKeyError(e instanceof Error ? e.message : "Failed to revoke key");
    }
  };

  const vpnHasRedacted = Object.values(vpnEnvVars).some((v) => v === "***");

  const handleVpnSave = async () => {
    if (!vpnHasRedacted) {
      if (vpnType === "openvpn") {
        if (!vpnEnvVars["OPENVPN_USER"]?.trim()) {
          setVpnError("Please enter your OpenVPN username.");
          return;
        }
        if (!vpnEnvVars["OPENVPN_PASSWORD"]?.trim()) {
          setVpnError("Please enter your OpenVPN password.");
          return;
        }
      } else if (vpnType === "wireguard") {
        if (!vpnEnvVars["WIREGUARD_PRIVATE_KEY"]?.trim()) {
          setVpnError("Please enter your WireGuard private key.");
          return;
        }
      }
    }
    setVpnSaving(true);
    setVpnError(null);
    setVpnSaved(false);
    try {
      const cfg: VpnConfig = {
        enabled: true,
        provider: vpnProvider,
        vpn_type: vpnType,
        server_countries: vpnCountry || undefined,
        port_forwarding: vpnPortForward,
        env_vars: vpnHasRedacted ? {} : vpnEnvVars,
      };
      await api.saveVpnConfig(cfg);
      setVpnConfig(cfg);
      setVpnSaved(true);
      setTimeout(() => setVpnSaved(false), 3000);
    } catch (e: unknown) {
      setVpnError(e instanceof Error ? e.message : "Failed to save VPN config");
    } finally {
      setVpnSaving(false);
    }
  };

  const handleVpnRemove = async () => {
    setVpnSaving(true);
    setVpnError(null);
    try {
      await api.saveVpnConfig({ enabled: false });
      setVpnConfig(null);
      setVpnProvider("protonvpn");
      setVpnType("openvpn");
      setVpnCountry("");
      setVpnPortForward(true);
      setVpnEnvVars({});
    } catch (e: unknown) {
      setVpnError(e instanceof Error ? e.message : "Failed to remove VPN config");
    } finally {
      setVpnSaving(false);
    }
  };

  const copyKey = async () => {
    if (!newRawKey) return;
    try {
      await navigator.clipboard.writeText(newRawKey);
      setKeyCopied(true);
      setTimeout(() => setKeyCopied(false), 2000);
    } catch {
      // fallback: select the text
    }
  };

  if (!config) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading settings...</p>
      </div>
    );
  }

  return (
    <div class="flex-1 px-6 py-6 max-w-4xl mx-auto w-full">
      <h1 class="text-xl font-bold mb-6">Settings</h1>

      {/* Default Storage Path */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Default Storage Path
        </h2>
        <p class="text-xs text-gray-500 mb-3">
          New apps will store data under this path. Leave empty to use
          ~/.myground/apps/.
        </p>
        <div class="flex gap-2 items-center mb-2">
          <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 font-mono truncate min-w-0">
            {config.default_storage_path || "~/.myground/apps/ (default)"}
          </span>
          <button
            class="px-3 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
            onClick={() => setEditingPath(!editingPath)}
          >
            {editingPath ? "Cancel" : "Browse"}
          </button>
          {config.default_storage_path && (
            <button
              class="px-3 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
              onClick={() =>
                setConfig({ ...config, default_storage_path: undefined })
              }
            >
              Clear
            </button>
          )}
        </div>
        {editingPath && (
          <PathPicker
            initialPath={config.default_storage_path || "/"}
            onSelect={(path) => {
              setConfig({ ...config, default_storage_path: path });
              setEditingPath(false);
            }}
            onCancel={() => setEditingPath(false)}
          />
        )}
      </section>

      {/* Global Backup Config */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Global Backup Defaults
        </h2>
        <p class="text-xs text-gray-500 mb-3">
          Default backup settings used when initializing app backups.
        </p>
        <div class="space-y-4">
          <AwsSetupForm
            currentRepository={config.backup?.repository}
            onSuccess={(result: AwsSetupResult) => {
              if (!config) return;
              setConfig({
                ...config,
                backup: {
                  ...config.backup,
                  repository: result.repository,
                  s3_access_key: result.s3_access_key,
                  s3_secret_key: result.s3_secret_key,
                },
              });
            }}
          />
          <details class="group">
            <summary class="text-xs text-gray-500 cursor-pointer hover:text-gray-400">
              Advanced / Manual setup
            </summary>
            <div class="mt-3 space-y-3">
              <Field
                label="Repository"
                type="text"
                value={config.backup?.repository ?? ""}
                placeholder="/mnt/backups"
                onInput={(v) => updateBackup("repository", v)}
              />
              <Field
                label="S3 Access Key"
                type="text"
                value={config.backup?.s3_access_key ?? ""}
                onInput={(v) => updateBackup("s3_access_key", v)}
              />
              <Field
                label="S3 Secret Key"
                type="password"
                value={config.backup?.s3_secret_key ?? ""}
                onInput={(v) => updateBackup("s3_secret_key", v)}
              />
              <p class="text-xs text-gray-500">
                Encryption passwords are generated automatically per app.
              </p>
            </div>
          </details>
        </div>
      </section>

      {/* Global VPN Config */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Global VPN
        </h2>
        <p class="text-xs text-gray-500 mb-3">
          Configure your VPN provider once — apps can enable it with a single toggle.
        </p>
        {vpnConfig?.provider ? (
          <div class="space-y-3">
            <div class="bg-gray-800 border border-gray-700 rounded p-3">
              <p class="text-sm text-gray-200">
                Provider: <span class="text-amber-400">{vpnConfig.provider}</span>
                {vpnConfig.vpn_type && <span class="text-gray-500"> ({vpnConfig.vpn_type})</span>}
              </p>
              {vpnConfig.server_countries && (
                <p class="text-xs text-gray-400 mt-1">Country: {vpnConfig.server_countries}</p>
              )}
              <p class="text-xs text-gray-400 mt-1">
                Port forwarding: {vpnConfig.port_forwarding ? "on" : "off"}
              </p>
            </div>
            <div class="flex gap-2">
              <button
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                onClick={() => {
                  setVpnConfig(null);
                }}
              >
                Edit
              </button>
              <button
                class="px-3 py-1.5 bg-red-900/50 hover:bg-red-800/50 text-red-400 text-xs rounded disabled:opacity-50"
                disabled={vpnSaving}
                onClick={handleVpnRemove}
              >
                {vpnSaving ? "..." : "Remove"}
              </button>
            </div>
          </div>
        ) : (
          <div class="space-y-3">
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
                Leave this on unless you know you don't need it.
              </p>
            </div>
            {vpnHasRedacted && (
              <p class="text-xs text-amber-400">
                Credentials are redacted. Re-enter them to update.
              </p>
            )}
            <div class="flex items-center gap-3">
              <button
                class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
                onClick={handleVpnSave}
                disabled={vpnSaving}
              >
                {vpnSaving ? "Saving..." : "Save VPN Config"}
              </button>
              {vpnSaved && <span class="text-green-400 text-sm">Saved</span>}
              {vpnError && <span class="text-red-400 text-sm">{vpnError}</span>}
            </div>
          </div>
        )}
      </section>

      {/* Save */}
      <div class="flex items-center gap-3">
        <button
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
          onClick={save}
          disabled={saving}
        >
          {saving ? "Saving..." : "Save"}
        </button>
        {saved && <span class="text-green-400 text-sm">Saved</span>}
        {error && <span class="text-red-400 text-sm">{error}</span>}
      </div>

      {/* API Keys */}
      <section class="mt-8 pt-8 border-t border-gray-800">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          API Keys
        </h2>
        <p class="text-xs text-gray-500 mb-4">
          Create API keys for CLI authentication and scripting. Keys are shown
          only once.
        </p>

        {/* New key form */}
        <div class="flex gap-2 mb-4">
          <input
            type="text"
            value={newKeyName}
            onInput={(e) =>
              setNewKeyName((e.target as HTMLInputElement).value)
            }
            placeholder="Key name (e.g. laptop, CI)"
            class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
          />
          <button
            class="px-4 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50 shrink-0"
            onClick={handleCreateKey}
            disabled={creatingKey || !newKeyName.trim()}
          >
            {creatingKey ? "Creating..." : "Create"}
          </button>
        </div>

        {/* One-time key display */}
        {newRawKey && (
          <div class="mb-4 p-3 bg-gray-800 border border-amber-700/50 rounded">
            <p class="text-xs text-amber-400 mb-2">
              Copy this key now — it won't be shown again.
            </p>
            <div class="flex gap-2 items-center">
              <code class="flex-1 text-xs text-gray-200 font-mono break-all select-all">
                {newRawKey}
              </code>
              <button
                class="px-3 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded shrink-0"
                onClick={copyKey}
              >
                {keyCopied ? "Copied" : "Copy"}
              </button>
            </div>
          </div>
        )}

        {keyError && (
          <p class="text-red-400 text-sm mb-3">{keyError}</p>
        )}

        {/* Existing keys */}
        {apiKeys.length > 0 ? (
          <div class="space-y-2">
            {apiKeys.map((k) => (
              <div
                key={k.id}
                class="flex items-center justify-between bg-gray-800/50 border border-gray-700/50 rounded px-3 py-2"
              >
                <div class="min-w-0">
                  <span class="text-sm text-gray-200">{k.name}</span>
                  <span class="text-xs text-gray-500 ml-2">
                    {formatTimestamp(k.created_at)}
                  </span>
                </div>
                <button
                  class="px-3 py-1 bg-red-900/50 hover:bg-red-800/50 text-red-400 text-xs rounded shrink-0"
                  onClick={() => handleRevokeKey(k.id)}
                >
                  Revoke
                </button>
              </div>
            ))}
          </div>
        ) : (
          <p class="text-xs text-gray-500">No API keys created yet.</p>
        )}
      </section>

      {/* Account */}
      <section class="mt-8 pt-8 border-t border-gray-800">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Account
        </h2>
        {onLogout && (
          <button
            class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
            onClick={onLogout}
          >
            Logout
          </button>
        )}
      </section>
    </div>
  );
}
