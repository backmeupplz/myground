import { useState, useEffect } from "preact/hooks";
import {
  api,
  formatTimestamp,
  type GlobalConfig,
  type ApiKeyInfo,
  type UpdateStatus,
  type UpdateConfig,
} from "../api";
import { PathPicker } from "../components/path-picker";
import { Field } from "../components/field";

interface Props {
  onLogout?: () => void;
}

export function Settings({ onLogout }: Props) {
  const [config, setConfig] = useState<GlobalConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingPath, setEditingPath] = useState(false);

  // Updates state
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [updateConfig, setUpdateConfig] = useState<UpdateConfig | null>(null);
  const [checking, setChecking] = useState(false);
  const [selfUpdating, setSelfUpdating] = useState(false);

  // API Keys state
  const [apiKeys, setApiKeys] = useState<ApiKeyInfo[]>([]);
  const [newKeyName, setNewKeyName] = useState("");
  const [creatingKey, setCreatingKey] = useState(false);
  const [newRawKey, setNewRawKey] = useState<string | null>(null);
  const [keyCopied, setKeyCopied] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  useEffect(() => {
    api.globalConfig().then(setConfig).catch(() => {});
    api.listApiKeys().then(setApiKeys).catch(() => {});
    api.updateStatus().then(setUpdateStatus).catch(() => {});
    api.updateConfig().then(setUpdateConfig).catch(() => {});
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
    <div class="flex-1 px-6 py-6 max-w-3xl mx-auto">
      <h1 class="text-xl font-bold mb-6">Settings</h1>

      {/* Default Storage Path */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Default Storage Path
        </h2>
        <p class="text-xs text-gray-500 mb-3">
          New apps will store data under this path. Leave empty to use
          ~/.myground/services/.
        </p>
        <div class="flex gap-2 items-center mb-2">
          <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 font-mono truncate min-w-0">
            {config.default_storage_path || "~/.myground/services/ (default)"}
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
        <div class="space-y-3">
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

      {/* Updates */}
      <section class="mt-8 pt-8 border-t border-gray-800">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Updates
        </h2>

        {/* MyGround version */}
        <div class="flex items-center justify-between mb-4">
          <div>
            <span class="text-sm text-gray-200">MyGround</span>
            <span class="text-xs text-gray-500 ml-2">
              v{updateStatus?.myground_version ?? "..."}
            </span>
            {updateStatus?.myground_update_available && (
              <span class="ml-2 text-xs text-blue-400">
                v{updateStatus.latest_myground_version} available
              </span>
            )}
          </div>
          {updateStatus?.myground_update_available && (
            <button
              class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
              disabled={selfUpdating}
              onClick={async () => {
                setSelfUpdating(true);
                try {
                  await api.selfUpdate();
                } catch {
                  setSelfUpdating(false);
                }
              }}
            >
              {selfUpdating ? "Updating..." : "Upgrade"}
            </button>
          )}
        </div>

        {/* Auto-update toggles */}
        {updateConfig && (
          <div class="space-y-3 mb-4">
            <label class="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={updateConfig.auto_update_services}
                onChange={async (e) => {
                  const val = (e.target as HTMLInputElement).checked;
                  const newCfg = { ...updateConfig, auto_update_services: val };
                  setUpdateConfig(newCfg);
                  await api.saveUpdateConfig({
                    auto_update_services: val,
                    auto_update_myground: newCfg.auto_update_myground,
                  });
                }}
                class="rounded"
              />
              <span class="text-sm text-gray-300">Auto-update apps</span>
            </label>
            <label class="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={updateConfig.auto_update_myground}
                onChange={async (e) => {
                  const val = (e.target as HTMLInputElement).checked;
                  const newCfg = { ...updateConfig, auto_update_myground: val };
                  setUpdateConfig(newCfg);
                  await api.saveUpdateConfig({
                    auto_update_services: newCfg.auto_update_services,
                    auto_update_myground: val,
                  });
                }}
                class="rounded"
              />
              <span class="text-sm text-gray-300">Auto-update MyGround</span>
            </label>
          </div>
        )}

        {/* Check now */}
        <div class="flex items-center gap-3">
          <button
            class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50"
            disabled={checking}
            onClick={async () => {
              setChecking(true);
              try {
                await api.updateCheck();
                // Poll for results after a delay
                setTimeout(async () => {
                  const status = await api.updateStatus();
                  setUpdateStatus(status);
                  setChecking(false);
                }, 5000);
              } catch {
                setChecking(false);
              }
            }}
          >
            {checking ? "Checking..." : "Check Now"}
          </button>
          {updateStatus?.last_check && (
            <span class="text-xs text-gray-500">
              Last checked: {formatTimestamp(updateStatus.last_check)}
            </span>
          )}
        </div>
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
