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

function validateCronField(field: string, min: number, max: number): boolean {
  if (field === "*") return true;
  // */N step
  if (/^\*\/\d+$/.test(field)) {
    const step = parseInt(field.slice(2), 10);
    return step >= 1 && step <= max;
  }
  // comma-separated list of values or ranges
  return field.split(",").every((part) => {
    const range = part.split("-");
    if (range.length === 2) {
      const [a, b] = range.map((s) => parseInt(s, 10));
      return !isNaN(a) && !isNaN(b) && a >= min && b <= max && a <= b;
    }
    if (range.length === 1) {
      const n = parseInt(range[0], 10);
      return !isNaN(n) && n >= min && n <= max;
    }
    return false;
  });
}

function validateCron(expr: string): string | null {
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return "Must have 5 fields: min hour day month weekday";
  const [min, hour, day, month, weekday] = parts;
  if (!validateCronField(min, 0, 59)) return "Invalid minute field (0-59)";
  if (!validateCronField(hour, 0, 23)) return "Invalid hour field (0-23)";
  if (!validateCronField(day, 1, 31)) return "Invalid day field (1-31)";
  if (!validateCronField(month, 1, 12)) return "Invalid month field (1-12)";
  if (!validateCronField(weekday, 0, 6)) return "Invalid weekday field (0-6, Sun=0)";
  return null;
}

const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS = ["", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

function describeCronField(field: string, names?: string[]): string {
  if (field === "*") return "every";
  if (field.startsWith("*/")) return `every ${field.slice(2)}`;
  const parts = field.split(",").map((p) => {
    if (p.includes("-")) {
      const [a, b] = p.split("-");
      const na = names ? names[parseInt(a, 10)] : a;
      const nb = names ? names[parseInt(b, 10)] : b;
      return `${na}-${nb}`;
    }
    return names ? names[parseInt(p, 10)] : p;
  });
  return parts.join(", ");
}

function describeCron(expr: string): string | null {
  if (validateCron(expr)) return null;
  const [min, hour, day, month, weekday] = expr.trim().split(/\s+/);

  const parts: string[] = [];

  // Time
  const minDesc = min === "*" ? "every minute" : min.startsWith("*/") ? `every ${min.slice(2)} minutes` : null;
  const hourDesc = hour === "*" ? null : hour.startsWith("*/") ? `every ${hour.slice(2)} hours` : null;

  if (minDesc && hour === "*") {
    parts.push(minDesc);
  } else if (min !== "*" && hour !== "*" && !min.startsWith("*/") && !hour.startsWith("*/")) {
    const hours = hour.split(",").map((h) => {
      const mins = min.split(",").map((m) => m.padStart(2, "0"));
      return mins.map((m) => `${h.padStart(2, "0")}:${m}`).join(", ");
    });
    parts.push(`at ${hours.join(", ")} UTC`);
  } else {
    if (minDesc) parts.push(`minute: ${minDesc}`);
    else if (min !== "0" && min !== "*") parts.push(`at minute ${min}`);
    if (hourDesc) parts.push(hourDesc);
    else if (hour !== "*" && !parts.some((p) => p.includes(":"))) parts.push(`at hour ${hour} UTC`);
  }

  // Day of month
  if (day !== "*") {
    const dayDesc = describeCronField(day);
    parts.push(day.startsWith("*/") ? `every ${day.slice(2)} days` : `on day ${dayDesc}`);
  }

  // Month
  if (month !== "*") {
    parts.push(`in ${describeCronField(month, MONTHS)}`);
  }

  // Weekday
  if (weekday !== "*") {
    parts.push(`on ${describeCronField(weekday, WEEKDAYS)}`);
  }

  return parts.join(", ") || "every minute";
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
            <div class="mt-2 space-y-1">
              <Field
                label="Cron expression (min hour day month weekday)"
                type="text"
                value={scheduleValue}
                placeholder="0 2 * * *"
                onInput={(v) => onChange({ ...config, schedule: v })}
              />
              {(() => {
                const err = validateCron(scheduleValue);
                if (err) return <p class="text-xs text-red-400">{err}</p>;
                const desc = describeCron(scheduleValue);
                if (desc) return <p class="text-xs text-gray-500">{desc}</p>;
                return null;
              })()}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
