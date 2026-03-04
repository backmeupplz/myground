import { useState, useEffect } from "preact/hooks";
import { api, type AppBackupConfig, type BackupConfig, type AwsSetupResult } from "../api";
import { PathPicker } from "./path-picker";
import { Field } from "./field";
import { AwsSetupForm } from "./aws-setup-form";
import {
  isCustomCron,
  validateCron,
  describeCron,
} from "../utils/cron";

interface Props {
  config: AppBackupConfig;
  onChange: (config: AppBackupConfig) => void;
}

const SCHEDULE_PRESETS = [
  { value: "", label: "Manual only" },
  { value: "daily", label: "Daily (2 AM UTC)" },
  { value: "weekly", label: "Weekly (Sun 2 AM UTC)" },
  { value: "monthly", label: "Monthly (1st, 2 AM UTC)" },
  { value: "custom", label: "Custom (cron)" },
];

function updateNested(
  base: BackupConfig | undefined,
  key: string,
  value: string,
): BackupConfig {
  return { ...base, [key]: value };
}

export function BackupConfigFields({ config, onChange }: Props) {
  const [editingPathIndex, setEditingPathIndex] = useState<number | null>(null);
  const [globalBackup, setGlobalBackup] = useState<BackupConfig | null>(null);

  useEffect(() => {
    api.backupConfig().then(setGlobalBackup).catch(() => {});
  }, []);

  const hasAnyBackup = config.local.length > 0 || config.remote.length > 0;
  const scheduleValue = config.schedule || "";
  const showCustomInput = isCustomCron(scheduleValue);

  const updateLocal = (index: number, updated: BackupConfig) => {
    const next = [...config.local];
    next[index] = updated;
    onChange({ ...config, local: next });
  };

  const removeLocal = (index: number) => {
    const next = config.local.filter((_, i) => i !== index);
    onChange({ ...config, local: next, enabled: next.length > 0 });
    if (editingPathIndex === index) setEditingPathIndex(null);
  };

  const addLocal = () => {
    onChange({ ...config, local: [...config.local, {}], enabled: true });
  };

  const updateRemote = (index: number, updated: BackupConfig) => {
    const next = [...config.remote];
    next[index] = updated;
    onChange({ ...config, remote: next });
  };

  const removeRemote = (index: number) => {
    onChange({ ...config, remote: config.remote.filter((_, i) => i !== index) });
  };

  const addRemote = () => {
    const defaults: BackupConfig = {};
    if (globalBackup?.repository) defaults.repository = globalBackup.repository;
    if (globalBackup?.s3_access_key) defaults.s3_access_key = globalBackup.s3_access_key;
    if (globalBackup?.s3_secret_key) defaults.s3_secret_key = globalBackup.s3_secret_key;
    onChange({ ...config, remote: [...config.remote, defaults] });
  };

  return (
    <div class="space-y-4">
      <div class="flex gap-2 bg-gray-800/50 rounded p-3">
        <span class="text-blue-400 shrink-0" aria-hidden="true">&#9432;</span>
        <div class="text-xs text-gray-400 space-y-1">
          <p>
            Backups are <strong class="text-gray-300">incremental</strong> (only changed data is stored) and <strong class="text-gray-300">encrypted</strong> (protected even if someone accesses the storage).
          </p>
          <p class="text-amber-400">
            Keep your backup password safe — without it, backups cannot be restored.
          </p>
        </div>
      </div>

      {/* Local backups */}
      <div class="space-y-3">
        <h3 class="text-sm text-gray-400 font-medium">Local Backups</h3>
        {config.local.map((entry, i) => (
          <div key={i} class="pl-4 border-l-2 border-gray-700 space-y-2">
            <div class="flex items-center justify-between">
              <span class="text-xs text-gray-500">Local #{i + 1}</span>
              <button
                type="button"
                class="text-xs text-red-400 hover:text-red-300"
                onClick={() => removeLocal(i)}
              >
                Remove
              </button>
            </div>
            <div>
              <label class="text-xs text-gray-500 block mb-1">
                Repository path
              </label>
              <div class="flex gap-2 items-center">
                <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate min-w-0">
                  {entry.repository || "/mnt/backups"}
                </span>
                <button
                  type="button"
                  class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                  onClick={() => setEditingPathIndex(editingPathIndex === i ? null : i)}
                >
                  {editingPathIndex === i ? "Cancel" : "Browse"}
                </button>
              </div>
              {editingPathIndex === i && (
                <div class="mt-2">
                  <PathPicker
                    initialPath={entry.repository || "/"}
                    onSelect={(path) => {
                      updateLocal(i, updateNested(entry, "repository", path));
                      setEditingPathIndex(null);
                    }}
                    onCancel={() => setEditingPathIndex(null)}
                  />
                </div>
              )}
            </div>
          </div>
        ))}
        <button
          type="button"
          class="text-sm text-blue-400 hover:text-blue-300"
          onClick={addLocal}
        >
          + Add local backup
        </button>
      </div>

      {/* Remote (S3) backups */}
      <div class="space-y-3">
        <h3 class="text-sm text-gray-400 font-medium">Cloud Backups (S3)</h3>
        {config.remote.map((entry, i) => (
          <div key={i} class="pl-4 border-l-2 border-gray-700 space-y-2">
            <div class="flex items-center justify-between">
              <span class="text-xs text-gray-500">S3 #{i + 1}</span>
              <button
                type="button"
                class="text-xs text-red-400 hover:text-red-300"
                onClick={() => removeRemote(i)}
              >
                Remove
              </button>
            </div>
            <AwsSetupForm
              currentRepository={entry.repository}
              onSuccess={(result: AwsSetupResult) => {
                updateRemote(i, {
                  ...entry,
                  repository: result.repository,
                  s3_access_key: result.s3_access_key,
                  s3_secret_key: result.s3_secret_key,
                });
              }}
            />
            <details class="group">
              <summary class="text-xs text-gray-500 cursor-pointer hover:text-gray-400">
                Advanced / Manual setup
              </summary>
              <div class="mt-3 space-y-3">
                <Field
                  label="Bucket URL"
                  type="text"
                  value={entry.repository ?? ""}
                  placeholder="s3:https://s3.amazonaws.com/mybucket"
                  onInput={(v) => updateRemote(i, updateNested(entry, "repository", v))}
                />
                <Field
                  label="Access Key"
                  type="text"
                  value={entry.s3_access_key ?? ""}
                  onInput={(v) => updateRemote(i, updateNested(entry, "s3_access_key", v))}
                />
                <Field
                  label="Secret Key"
                  type="password"
                  value={entry.s3_secret_key ?? ""}
                  onInput={(v) => updateRemote(i, updateNested(entry, "s3_secret_key", v))}
                />
              </div>
            </details>
          </div>
        ))}
        <button
          type="button"
          class="text-sm text-blue-400 hover:text-blue-300"
          onClick={addRemote}
        >
          + Add cloud backup
        </button>
      </div>

      {/* Schedule — only show when at least one backup method is configured */}
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
