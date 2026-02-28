import { useState, useEffect } from "preact/hooks";
import { api, type GlobalConfig, type BackupConfig } from "../api";
import { PathPicker } from "../components/path-picker";

function Field({
  label,
  type,
  value,
  placeholder,
  onInput,
}: {
  label: string;
  type: string;
  value: string;
  placeholder?: string;
  onInput: (value: string) => void;
}) {
  return (
    <div>
      <label class="text-xs text-gray-500 block mb-1">{label}</label>
      <input
        type={type}
        value={value}
        onInput={(e) => onInput((e.target as HTMLInputElement).value)}
        class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
        placeholder={placeholder}
      />
    </div>
  );
}

export function Settings() {
  const [config, setConfig] = useState<GlobalConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingPath, setEditingPath] = useState(false);

  useEffect(() => {
    api.globalConfig().then(setConfig).catch(() => {});
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

  const updateBackupNumber = (key: string, value: string) => {
    if (!config) return;
    const num = value === "" ? undefined : parseInt(value, 10);
    setConfig({
      ...config,
      backup: { ...config.backup, [key]: num } as BackupConfig,
    });
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
          New services will store data under this path. Leave empty to use
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
              onClick={() => setConfig({ ...config, default_storage_path: undefined })}
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
          Default backup settings used when initializing service backups.
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
            label="Password"
            type="password"
            value={config.backup?.password ?? ""}
            placeholder="Encryption password"
            onInput={(v) => updateBackup("password", v)}
          />
          <div class="grid grid-cols-3 gap-3">
            <Field
              label="Keep daily"
              type="number"
              value={config.backup?.keep_daily?.toString() ?? ""}
              placeholder="7"
              onInput={(v) => updateBackupNumber("keep_daily", v)}
            />
            <Field
              label="Keep weekly"
              type="number"
              value={config.backup?.keep_weekly?.toString() ?? ""}
              placeholder="4"
              onInput={(v) => updateBackupNumber("keep_weekly", v)}
            />
            <Field
              label="Keep monthly"
              type="number"
              value={config.backup?.keep_monthly?.toString() ?? ""}
              placeholder="6"
              onInput={(v) => updateBackupNumber("keep_monthly", v)}
            />
          </div>
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
    </div>
  );
}
