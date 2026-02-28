import type { ServiceBackupConfig, BackupConfig } from "../api";

interface Props {
  config: ServiceBackupConfig;
  onChange: (config: ServiceBackupConfig) => void;
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

function updateNested(
  base: BackupConfig | undefined,
  key: string,
  value: string,
): BackupConfig {
  return { ...base, [key]: value };
}

export function BackupConfigFields({ config, onChange }: Props) {
  return (
    <div class="space-y-4">
      <label class="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={config.enabled}
          onChange={(e) =>
            onChange({
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
              onChange({
                ...config,
                local: updateNested(config.local, "repository", v),
              })
            }
          />
          <Field
            label="Password"
            type="password"
            value={config.local?.password ?? ""}
            onInput={(v) =>
              onChange({
                ...config,
                local: updateNested(config.local, "password", v),
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
            onChange({ ...config, remote: checked ? {} : undefined });
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
              onChange({
                ...config,
                remote: updateNested(config.remote, "repository", v),
              })
            }
          />
          <Field
            label="Access Key"
            type="text"
            value={config.remote.s3_access_key ?? ""}
            onInput={(v) =>
              onChange({
                ...config,
                remote: updateNested(config.remote, "s3_access_key", v),
              })
            }
          />
          <Field
            label="Secret Key"
            type="password"
            value={config.remote.s3_secret_key ?? ""}
            onInput={(v) =>
              onChange({
                ...config,
                remote: updateNested(config.remote, "s3_secret_key", v),
              })
            }
          />
          <Field
            label="Password"
            type="password"
            value={config.remote.password ?? ""}
            onInput={(v) =>
              onChange({
                ...config,
                remote: updateNested(config.remote, "password", v),
              })
            }
          />
        </div>
      )}
    </div>
  );
}
