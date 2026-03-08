import { useState, useEffect } from "preact/hooks";
import {
  api,
  formatTimestamp,
  type GlobalConfig,
  type ApiKeyInfo,
  type VpnConfig,
  type AwsSetupResult,
  type BackupConfig,
  type VerifyResult,
} from "../api";
import { PathPicker } from "../components/path-picker";
import { Field } from "../components/field";
import { AwsSetupForm } from "../components/aws-setup-form";
import { VpnConfigForm } from "../components/vpn-config-form";

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
  const [vpnTesting, setVpnTesting] = useState(false);
  const [vpnTestResult, setVpnTestResult] = useState<boolean | null>(null);
  const [vpnTestLogs, setVpnTestLogs] = useState<string[]>([]);

  useEffect(() => {
    api.globalConfig().then(setConfig).catch((e) => console.warn("Failed to load config:", e));
    api.listApiKeys().then(setApiKeys).catch((e) => console.warn("Failed to load API keys:", e));
    api.getVpnConfig().then((cfg) => {
      setVpnConfig(cfg);
      if (cfg.provider) setVpnProvider(cfg.provider);
      if (cfg.vpn_type) setVpnType(cfg.vpn_type);
      if (cfg.server_countries) setVpnCountry(cfg.server_countries);
      setVpnPortForward(cfg.port_forwarding ?? true);
      if (cfg.env_vars) setVpnEnvVars(cfg.env_vars);
    }).catch((e) => console.warn("Failed to load VPN config:", e));
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

  // Local destination state
  const [editingLocalPath, setEditingLocalPath] = useState(false);
  const [localVerifying, setLocalVerifying] = useState(false);
  const [localVerifyResult, setLocalVerifyResult] = useState<VerifyResult | null>(null);

  // Remote destination state
  const [remoteVerifying, setRemoteVerifying] = useState(false);
  const [remoteVerifyResult, setRemoteVerifyResult] = useState<VerifyResult | null>(null);

  const updateLocalDest = (key: string, value: string) => {
    if (!config) return;
    setConfig({
      ...config,
      default_local_destination: { ...config.default_local_destination, [key]: value || undefined },
    });
  };

  const updateRemoteDest = (key: string, value: string) => {
    if (!config) return;
    setConfig({
      ...config,
      default_remote_destination: { ...config.default_remote_destination, [key]: value || undefined },
    });
  };

  const verifyDest = async (dest: BackupConfig | undefined, setVerifying: (v: boolean) => void, setResult: (r: VerifyResult | null) => void) => {
    if (!dest?.repository) return;
    setVerifying(true);
    setResult(null);
    try {
      const result = await api.verifyBackup(dest);
      setResult(result);
    } catch (e) {
      setResult({ ok: false, error: e instanceof Error ? e.message : "Verification failed" });
    } finally {
      setVerifying(false);
    }
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
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full">
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

      {/* Default Local Destination */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Default Local Backup Destination
        </h2>
        <p class="text-xs text-gray-500 mb-3">
          Default path for local backups. New backup jobs will use this unless overridden.
        </p>
        <div class="space-y-3">
          <div class="flex gap-2 items-center">
            <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 font-mono truncate min-w-0">
              {config.default_local_destination?.repository || "~/.myground/backups/ (default)"}
            </span>
            <button
              class="px-3 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
              onClick={() => setEditingLocalPath(!editingLocalPath)}
            >
              {editingLocalPath ? "Cancel" : "Browse"}
            </button>
            {config.default_local_destination?.repository && (
              <button
                class="px-3 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                onClick={() => setConfig({ ...config, default_local_destination: undefined })}
              >
                Clear
              </button>
            )}
          </div>
          {editingLocalPath && (
            <PathPicker
              initialPath={config.default_local_destination?.repository || "/"}
              onSelect={(path) => {
                setConfig({ ...config, default_local_destination: { ...config.default_local_destination, repository: path } });
                setEditingLocalPath(false);
              }}
              onCancel={() => setEditingLocalPath(false)}
            />
          )}
          {config.default_local_destination?.repository && (
            <div class="flex items-center gap-2">
              <button
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50"
                disabled={localVerifying}
                onClick={() => verifyDest(config.default_local_destination, setLocalVerifying, setLocalVerifyResult)}
              >
                {localVerifying ? "Verifying..." : "Test Connection"}
              </button>
              {localVerifyResult && (
                <span class={`text-xs ${localVerifyResult.ok ? "text-green-400" : "text-red-400"}`}>
                  {localVerifyResult.ok
                    ? `Connected (${localVerifyResult.snapshot_count ?? 0} snapshots)`
                    : localVerifyResult.error}
                </span>
              )}
            </div>
          )}
        </div>
      </section>

      {/* Default Remote Destination */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Default Remote Backup Destination (S3)
        </h2>
        <p class="text-xs text-gray-500 mb-3">
          Default S3 destination for cloud backups. New backup jobs will use this unless overridden.
        </p>
        <div class="space-y-4">
          <AwsSetupForm
            currentRepository={config.default_remote_destination?.repository}
            onSuccess={(result: AwsSetupResult) => {
              if (!config) return;
              setConfig({
                ...config,
                default_remote_destination: {
                  ...config.default_remote_destination,
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
                value={config.default_remote_destination?.repository ?? ""}
                placeholder="s3:https://s3.amazonaws.com/mybucket"
                onInput={(v) => updateRemoteDest("repository", v)}
              />
              <Field
                label="S3 Access Key"
                type="text"
                value={config.default_remote_destination?.s3_access_key ?? ""}
                onInput={(v) => updateRemoteDest("s3_access_key", v)}
              />
              <Field
                label="S3 Secret Key"
                type="password"
                value={config.default_remote_destination?.s3_secret_key ?? ""}
                onInput={(v) => updateRemoteDest("s3_secret_key", v)}
              />
              <p class="text-xs text-gray-500">
                Encryption passwords are generated automatically per app.
              </p>
            </div>
          </details>
          {config.default_remote_destination?.repository && (
            <div class="flex items-center gap-2">
              <button
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50"
                disabled={remoteVerifying}
                onClick={() => verifyDest(config.default_remote_destination, setRemoteVerifying, setRemoteVerifyResult)}
              >
                {remoteVerifying ? "Verifying..." : "Test Connection"}
              </button>
              {remoteVerifyResult && (
                <span class={`text-xs ${remoteVerifyResult.ok ? "text-green-400" : "text-red-400"}`}>
                  {remoteVerifyResult.ok
                    ? `Connected (${remoteVerifyResult.snapshot_count ?? 0} snapshots)`
                    : remoteVerifyResult.error}
                </span>
              )}
            </div>
          )}
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
            <div class="flex gap-2 items-center flex-wrap">
              <button
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50"
                disabled={vpnTesting}
                onClick={async () => {
                  setVpnTesting(true);
                  setVpnTestResult(null);
                  setVpnTestLogs([]);
                  const ok = await api.testVpn(undefined, (line) =>
                    setVpnTestLogs((prev) => [...prev, line])
                  );
                  setVpnTestResult(ok);
                  setVpnTesting(false);
                }}
              >
                {vpnTesting ? "Testing..." : "Test Connection"}
              </button>
              <button
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                onClick={() => {
                  setVpnConfig(null);
                  setVpnTestResult(null);
                  setVpnTestLogs([]);
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
              {vpnTestResult !== null && (
                <span class={`text-xs ${vpnTestResult ? "text-green-400" : "text-red-400"}`}>
                  {vpnTestResult ? "Connected" : "Failed"}
                </span>
              )}
            </div>
            {vpnTestLogs.length > 0 && (
              <pre class="mt-2 p-3 bg-gray-950 rounded text-xs text-gray-400 font-mono max-h-48 overflow-y-auto whitespace-pre-wrap">
                {vpnTestLogs.join("\n")}
              </pre>
            )}
          </div>
        ) : (
          <VpnConfigForm
            vpnProvider={vpnProvider}
            vpnType={vpnType}
            vpnCountry={vpnCountry}
            vpnPortForward={vpnPortForward}
            vpnEnvVars={vpnEnvVars}
            vpnSaving={vpnSaving}
            vpnError={vpnError}
            onProviderChange={setVpnProvider}
            onTypeChange={setVpnType}
            onCountryChange={setVpnCountry}
            onPortForwardChange={setVpnPortForward}
            onEnvVarsChange={setVpnEnvVars}
            saveLabel="Save VPN Config"
            savingLabel="Saving..."
            vpnHasRedacted={vpnHasRedacted}
            vpnSaved={vpnSaved}
            vpnTesting={vpnTesting}
            vpnTestResult={vpnTestResult}
            vpnTestLogs={vpnTestLogs}
            onSave={handleVpnSave}
            onTest={async () => {
              setVpnTesting(true);
              setVpnTestResult(null);
              setVpnTestLogs([]);
              setVpnError(null);
              const cfg: VpnConfig = vpnHasRedacted ? {} as VpnConfig : {
                enabled: true,
                provider: vpnProvider,
                vpn_type: vpnType,
                server_countries: vpnCountry || undefined,
                port_forwarding: vpnPortForward,
                env_vars: vpnEnvVars,
              };
              const ok = await api.testVpn(cfg.provider ? cfg : undefined, (line) =>
                setVpnTestLogs((prev) => [...prev, line])
              );
              setVpnTestResult(ok);
              setVpnTesting(false);
            }}
          />
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
