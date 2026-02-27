import { useState, useEffect } from "preact/hooks";
import { api, type ServiceBackupConfig, type BackupConfig } from "../api";

interface Props {
  serviceId: string;
}

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

function updateConfig(
  base: BackupConfig | undefined,
  key: string,
  value: string,
): BackupConfig {
  return { ...base, [key]: value };
}

export function BackupForm({ serviceId }: Props) {
  const [config, setConfig] = useState<ServiceBackupConfig>({
    enabled: false,
  });
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    api
      .getServiceBackup(serviceId)
      .then(setConfig)
      .catch(() => {});
  }, [serviceId]);

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    try {
      await api.updateServiceBackup(serviceId, config);
      setMessage("Saved");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="space-y-4">
      <label class="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={config.enabled}
          onChange={(e) =>
            setConfig({
              ...config,
              enabled: (e.target as HTMLInputElement).checked,
            })
          }
          class="rounded bg-gray-700 border-gray-600"
        />
        <span class="text-gray-300">Enable local backups</span>
      </label>

      {config.enabled && (
        <div class="pl-6 space-y-3">
          <Field
            label="Repository path"
            type="text"
            value={config.local?.repository ?? ""}
            placeholder="/mnt/backups"
            onInput={(v) =>
              setConfig({
                ...config,
                local: updateConfig(config.local, "repository", v),
              })
            }
          />
          <Field
            label="Password"
            type="password"
            value={config.local?.password ?? ""}
            onInput={(v) =>
              setConfig({
                ...config,
                local: updateConfig(config.local, "password", v),
              })
            }
          />
        </div>
      )}

      <label class="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={!!config.remote}
          onChange={(e) => {
            const checked = (e.target as HTMLInputElement).checked;
            setConfig({
              ...config,
              remote: checked ? {} : undefined,
            });
          }}
          class="rounded bg-gray-700 border-gray-600"
        />
        <span class="text-gray-300">Enable cloud backups (S3)</span>
      </label>

      {config.remote && (
        <div class="pl-6 space-y-3">
          <Field
            label="Bucket URL"
            type="text"
            value={config.remote.repository ?? ""}
            placeholder="s3:https://s3.amazonaws.com/mybucket"
            onInput={(v) =>
              setConfig({
                ...config,
                remote: updateConfig(config.remote, "repository", v),
              })
            }
          />
          <Field
            label="Access Key"
            type="text"
            value={config.remote.s3_access_key ?? ""}
            onInput={(v) =>
              setConfig({
                ...config,
                remote: updateConfig(config.remote, "s3_access_key", v),
              })
            }
          />
          <Field
            label="Secret Key"
            type="password"
            value={config.remote.s3_secret_key ?? ""}
            onInput={(v) =>
              setConfig({
                ...config,
                remote: updateConfig(config.remote, "s3_secret_key", v),
              })
            }
          />
          <Field
            label="Password"
            type="password"
            value={config.remote.password ?? ""}
            onInput={(v) =>
              setConfig({
                ...config,
                remote: updateConfig(config.remote, "password", v),
              })
            }
          />
        </div>
      )}

      <div class="flex items-center gap-3 pt-2">
        <button
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
          disabled={saving}
          onClick={handleSave}
        >
          {saving ? "Saving..." : "Save"}
        </button>
        {message && (
          <span
            class={`text-sm ${message === "Saved" ? "text-green-400" : "text-red-400"}`}
          >
            {message}
          </span>
        )}
      </div>
    </div>
  );
}
