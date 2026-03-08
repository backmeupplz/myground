import { useState, useEffect } from "preact/hooks";
import {
  api,
  type AppInfo,
  type BackupJobWithApp,
  type BackupConfig,
  type GlobalConfig,
  type VerifyResult,
} from "../api";
import { PathPicker } from "./path-picker";
import { Field } from "./field";
import {
  isCustomCron,
  validateCron,
  describeCron,
} from "../utils/cron";
import { SCHEDULE_PRESETS } from "../utils/backup";

interface JobDialogProps {
  apps: AppInfo[];
  editJob: BackupJobWithApp | null;
  onClose: () => void;
  onSaved: () => void;
  fixedAppId?: string;
  defaultDestType?: string;
}

export function JobDialog({ apps, editJob, onClose, onSaved, fixedAppId, defaultDestType }: JobDialogProps) {
  const isEdit = !!editJob;
  const [appId, setAppId] = useState(editJob?.app_id || fixedAppId || (apps[0]?.id ?? ""));
  const [destType, setDestType] = useState(editJob?.destination_type || defaultDestType || "remote");
  const [repository, setRepository] = useState(editJob?.repository || "");
  const [password, setPassword] = useState(editJob?.password || "");
  const [s3AccessKey, setS3AccessKey] = useState(editJob?.s3_access_key || "");
  const [s3SecretKey, setS3SecretKey] = useState(editJob?.s3_secret_key || "");
  const [schedule, setSchedule] = useState(editJob?.schedule || "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [editingPath, setEditingPath] = useState(false);
  const [verifying, setVerifying] = useState(false);
  const [verifyResult, setVerifyResult] = useState<VerifyResult | null>(null);
  const [globalCfg, setGlobalCfg] = useState<GlobalConfig | null>(null);
  const [overriding, setOverriding] = useState(!!editJob?.repository);

  useEffect(() => {
    if (!isEdit) {
      api.globalConfig().then(setGlobalCfg).catch((e) => console.warn("Failed to load global config:", e));
    }
  }, []);

  const showCustomCron = isCustomCron(schedule);

  // Determine if a default destination exists for the current dest type
  const defaultDest: BackupConfig | undefined =
    destType === "local"
      ? globalCfg?.default_local_destination
      : globalCfg?.default_remote_destination;
  const hasDefault = !!defaultDest?.repository;

  // Reset override state when switching dest type (only when creating)
  const handleDestTypeChange = (newType: string) => {
    setDestType(newType);
    if (!isEdit) {
      setOverriding(false);
      setRepository("");
      setS3AccessKey("");
      setS3SecretKey("");
      setEditingPath(false);
      setVerifyResult(null);
    }
  };

  const handleVerify = async () => {
    setVerifying(true);
    setVerifyResult(null);
    try {
      const config: BackupConfig = { repository: repository || undefined, password: password || undefined, s3_access_key: s3AccessKey || undefined, s3_secret_key: s3SecretKey || undefined };
      const result = await api.verifyBackup(config);
      setVerifyResult(result);
    } catch (e) {
      setVerifyResult({ ok: false, error: e instanceof Error ? e.message : "Verification failed" });
    } finally {
      setVerifying(false);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      if (isEdit && editJob) {
        await api.updateBackupJob(editJob.id, {
          destination_type: destType,
          repository: repository || undefined,
          password: password || undefined,
          s3_access_key: s3AccessKey || undefined,
          s3_secret_key: s3SecretKey || undefined,
          schedule: schedule || undefined,
        });
      } else {
        await api.createBackupJob({
          app_id: fixedAppId || appId,
          destination_type: destType,
          repository: repository || undefined,
          password: password || undefined,
          s3_access_key: s3AccessKey || undefined,
          s3_secret_key: s3SecretKey || undefined,
          schedule: schedule || undefined,
        });
      }
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
      <div class="bg-gray-900 rounded-lg max-w-lg w-full max-h-[90vh] overflow-y-auto p-6 space-y-4 text-left">
        <h2 class="text-lg font-bold text-gray-100">
          {isEdit ? "Edit Backup Job" : "Add Backup Job"}
        </h2>

        {/* App select — hidden when fixedAppId or editing */}
        {!isEdit && !fixedAppId && (
          <div>
            <label class="text-xs text-gray-500 block mb-1">App</label>
            <select
              value={appId}
              onChange={(e) => setAppId((e.target as HTMLSelectElement).value)}
              class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
            >
              {apps.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
          </div>
        )}

        {/* Destination type */}
        <div>
          <label class="text-xs text-gray-500 block mb-1">Destination Type</label>
          <div class="flex gap-2">
            <button
              type="button"
              class={`flex-1 px-3 py-2 text-sm rounded border ${destType === "local" ? "bg-blue-600 border-blue-500 text-white" : "bg-gray-800 border-gray-700 text-gray-400 hover:border-gray-600"}`}
              onClick={() => handleDestTypeChange("local")}
            >
              Local
            </button>
            <button
              type="button"
              class={`flex-1 px-3 py-2 text-sm rounded border ${destType === "remote" ? "bg-blue-600 border-blue-500 text-white" : "bg-gray-800 border-gray-700 text-gray-400 hover:border-gray-600"}`}
              onClick={() => handleDestTypeChange("remote")}
            >
              S3 (Remote)
            </button>
          </div>
        </div>

        {/* Default destination banner OR override fields */}
        {!isEdit && hasDefault && !overriding ? (
          <div class="bg-blue-900/20 border border-blue-500/30 rounded-lg p-4">
            <div class="flex items-center justify-between gap-3">
              <div class="min-w-0">
                <p class="text-sm text-blue-300">
                  Using default {destType === "local" ? "local" : "S3"} destination from Settings
                </p>
                <p class="text-xs text-gray-400 font-mono mt-1 truncate">
                  {defaultDest!.repository}
                </p>
              </div>
              <button
                type="button"
                class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded shrink-0"
                onClick={() => setOverriding(true)}
              >
                Override
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* Destination fields */}
            {destType === "local" ? (
              <div class="space-y-3">
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Repository path{!isEdit && hasDefault ? "" : " (optional override)"}
                  </label>
                  <div class="flex gap-2 items-center">
                    <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate min-w-0">
                      {repository || (hasDefault ? "Use default" : "~/.myground/backups/ (default)")}
                    </span>
                    <button
                      type="button"
                      class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                      onClick={() => setEditingPath(!editingPath)}
                    >
                      {editingPath ? "Cancel" : "Browse"}
                    </button>
                    {repository && (
                      <button
                        type="button"
                        class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                        onClick={() => setRepository("")}
                      >
                        Clear
                      </button>
                    )}
                  </div>
                  {editingPath && (
                    <div class="mt-2">
                      <PathPicker
                        initialPath={repository || "/"}
                        onSelect={(path) => {
                          setRepository(path);
                          setEditingPath(false);
                        }}
                        onCancel={() => setEditingPath(false)}
                      />
                    </div>
                  )}
                </div>
                {!isEdit && hasDefault && (
                  <button
                    type="button"
                    class="text-xs text-blue-400 hover:text-blue-300"
                    onClick={() => { setOverriding(false); setRepository(""); }}
                  >
                    Use default instead
                  </button>
                )}
              </div>
            ) : (
              <div class="space-y-3">
                <Field
                  label="Bucket URL (optional override)"
                  type="text"
                  value={repository}
                  placeholder="s3:https://s3.amazonaws.com/mybucket"
                  onInput={setRepository}
                />
                <Field
                  label="S3 Access Key"
                  type="text"
                  value={s3AccessKey}
                  onInput={setS3AccessKey}
                />
                <Field
                  label="S3 Secret Key"
                  type="password"
                  value={s3SecretKey}
                  onInput={setS3SecretKey}
                />
                {!isEdit && hasDefault && (
                  <button
                    type="button"
                    class="text-xs text-blue-400 hover:text-blue-300"
                    onClick={() => { setOverriding(false); setRepository(""); setS3AccessKey(""); setS3SecretKey(""); }}
                  >
                    Use default instead
                  </button>
                )}
              </div>
            )}

            {/* Verify */}
            {repository && (
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50"
                  disabled={verifying}
                  onClick={handleVerify}
                >
                  {verifying ? "Verifying..." : "Verify"}
                </button>
                {verifyResult && (
                  <span class={`text-xs ${verifyResult.ok ? "text-green-400" : "text-red-400"}`}>
                    {verifyResult.ok
                      ? `OK (${verifyResult.snapshot_count ?? 0} snapshots)`
                      : verifyResult.error}
                  </span>
                )}
              </div>
            )}
          </>
        )}

        {/* No default configured hint */}
        {!isEdit && !hasDefault && !repository && destType !== "local" && (
          <p class="text-xs text-gray-500">
            No default S3 destination configured.
            Set one in Settings or fill in the fields above.
          </p>
        )}

        {/* Schedule */}
        <div>
          <label class="text-xs text-gray-500 block mb-1">Schedule</label>
          <select
            value={showCustomCron ? "custom" : schedule}
            onChange={(e) => {
              const val = (e.target as HTMLSelectElement).value;
              setSchedule(val === "custom" ? "0 2 * * *" : val);
            }}
            class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
          >
            {SCHEDULE_PRESETS.map((p) => (
              <option key={p.value} value={p.value}>
                {p.label}
              </option>
            ))}
          </select>
          {showCustomCron && (
            <div class="mt-2 space-y-1">
              <Field
                label="Cron expression (min hour day month weekday)"
                type="text"
                value={schedule}
                placeholder="0 2 * * *"
                onInput={setSchedule}
              />
              {(() => {
                const err = validateCron(schedule);
                if (err) return <p class="text-xs text-red-400">{err}</p>;
                const desc = describeCron(schedule);
                if (desc) return <p class="text-xs text-gray-500">{desc}</p>;
                return null;
              })()}
            </div>
          )}
        </div>

        {error && <p class="text-red-400 text-sm">{error}</p>}

        <div class="flex gap-2 justify-end pt-2">
          <button
            type="button"
            class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
            disabled={saving || (!isEdit && !fixedAppId && !appId)}
            onClick={handleSave}
          >
            {saving ? "Saving..." : isEdit ? "Update" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
