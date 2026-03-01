import { useState } from "preact/hooks";
import type { ServiceBackupConfig, BackupConfig } from "../api";
import { PathPicker } from "./path-picker";

interface Props {
  config: ServiceBackupConfig;
  onChange: (config: ServiceBackupConfig) => void;
}

const SCHEDULE_PRESETS = [
  { value: "", label: "Manual only" },
  { value: "daily", label: "Daily (2 AM UTC)" },
  { value: "weekly", label: "Weekly (Sun 2 AM UTC)" },
  { value: "monthly", label: "Monthly (1st, 2 AM UTC)" },
  { value: "custom", label: "Custom (cron)" },
];

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

function isCustomCron(schedule: string | undefined): boolean {
  if (!schedule) return false;
  return !["", "daily", "weekly", "monthly"].includes(schedule);
}

export function BackupConfigFields({ config, onChange }: Props) {
  const [editingPath, setEditingPath] = useState(false);

  const hasAnyBackup = config.enabled || !!config.remote;
  const scheduleValue = config.schedule || "";
  const showCustomInput = isCustomCron(scheduleValue);

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
          <div>
            <label class="text-xs text-gray-500 block mb-1">
              Repository path
            </label>
            <div class="flex gap-2 items-center">
              <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate min-w-0">
                {config.local?.repository || "/mnt/backups"}
              </span>
              <button
                type="button"
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                onClick={() => setEditingPath(!editingPath)}
              >
                {editingPath ? "Cancel" : "Browse"}
              </button>
            </div>
            {editingPath && (
              <div class="mt-2">
                <PathPicker
                  initialPath={config.local?.repository || "/"}
                  onSelect={(path) => {
                    onChange({
                      ...config,
                      local: updateNested(config.local, "repository", path),
                    });
                    setEditingPath(false);
                  }}
                  onCancel={() => setEditingPath(false)}
                />
              </div>
            )}
          </div>
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
        </div>
      )}

      {/* Schedule — only show when at least one backup method is enabled */}
      {hasAnyBackup && (
        <div>
          <label class="text-xs text-gray-500 block mb-1">
            Backup schedule
          </label>
          <select
            value={showCustomInput ? "custom" : scheduleValue}
            onChange={(e) => {
              const val = (e.target as HTMLSelectElement).value;
              onChange({
                ...config,
                schedule: val === "custom" ? "0 2 * * *" : val || undefined,
              });
            }}
            class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
          >
            {SCHEDULE_PRESETS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
          {showCustomInput && (
            <div class="mt-2">
              <Field
                label="Cron expression (min hour day month weekday)"
                type="text"
                value={scheduleValue}
                placeholder="0 2 * * *"
                onInput={(v) => onChange({ ...config, schedule: v })}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
