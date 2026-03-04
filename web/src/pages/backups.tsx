import { useState, useEffect, useRef } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatTimestamp,
  formatBytes,
  type AppInfo,
  type BackupJobWithApp,
  type BackupJobProgress,
  type Snapshot,
  type BackupConfig,
  type VerifyResult,
} from "../api";
import { SnapshotRow } from "../components/snapshot-row";
import { PathPicker } from "../components/path-picker";
import { Field } from "../components/field";
import { AwsSetupForm } from "../components/aws-setup-form";
import {
  isCustomCron,
  validateCron,
  describeCron,
} from "../utils/cron";

interface Props {
  path?: string;
}

const SCHEDULE_PRESETS = [
  { value: "", label: "Manual only" },
  { value: "daily", label: "Daily (2 AM UTC)" },
  { value: "weekly", label: "Weekly (Sun 2 AM UTC)" },
  { value: "monthly", label: "Monthly (1st, 2 AM UTC)" },
  { value: "custom", label: "Custom (cron)" },
];

function scheduleLabel(schedule?: string): string {
  if (!schedule) return "Manual";
  const preset = SCHEDULE_PRESETS.find((p) => p.value === schedule);
  if (preset) return preset.label;
  const desc = describeCron(schedule);
  return desc || schedule;
}

function statusBadge(job: BackupJobWithApp): { text: string; color: string } {
  if (job.last_status === "succeeded") return { text: "Succeeded", color: "text-green-400" };
  if (job.last_status === "failed") return { text: "Failed", color: "text-red-400" };
  return { text: "Never run", color: "text-gray-500" };
}

function destBadge(job: BackupJobWithApp): { text: string; color: string } {
  if (job.destination_type === "local") return { text: "Local", color: "bg-blue-900/50 text-blue-400" };
  return { text: "S3", color: "bg-amber-900/50 text-amber-400" };
}

export function Backups({}: Props) {
  const [jobs, setJobs] = useState<BackupJobWithApp[]>([]);
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [progress, setProgress] = useState<Record<string, BackupJobProgress>>({});
  const [restoring, setRestoring] = useState<string | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingJob, setEditingJob] = useState<BackupJobWithApp | null>(null);
  const [runAllState, setRunAllState] = useState<"idle" | "running" | "done" | "error">("idle");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchData = async () => {
    try {
      const [jobList, appList] = await Promise.all([
        api.backupJobs(),
        api.apps(),
      ]);
      setJobs(jobList);
      setApps(appList.filter((a) => a.installed && a.backup_supported));

      // Fetch snapshots from all installed backup-capable apps
      const backupApps = appList.filter((a) => a.installed && a.backup_supported);
      const snapshotResults = await Promise.all(
        backupApps.map((a) =>
          api.appBackupSnapshots(a.id).catch(() => [] as Snapshot[]),
        ),
      );
      const seen = new Set<string>();
      const allSnaps: Snapshot[] = [];
      for (const snaps of snapshotResults) {
        for (const s of snaps) {
          if (!seen.has(s.id)) {
            seen.add(s.id);
            allSnaps.push(s);
          }
        }
      }
      allSnaps.sort((a, b) => b.time.localeCompare(a.time));
      setSnapshots(allSnaps);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  };

  // Poll for progress on running jobs
  const pollProgress = async () => {
    const running = jobs.filter(
      (j) => progress[j.id]?.status === "running",
    );
    if (running.length === 0) return;
    for (const j of running) {
      try {
        const p = await api.backupJobProgress(j.id);
        setProgress((prev) => ({ ...prev, [j.id]: p }));
        if (p.status !== "running") {
          // Refresh jobs to get updated last_status
          fetchData();
        }
      } catch {
        // Progress cleared = job done
        setProgress((prev) => {
          const next = { ...prev };
          delete next[j.id];
          return next;
        });
        fetchData();
      }
    }
  };

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 10000);
    return () => clearInterval(interval);
  }, []);

  // Fast poll when any job is running
  useEffect(() => {
    const hasRunning = Object.values(progress).some((p) => p.status === "running");
    if (hasRunning) {
      pollRef.current = setInterval(pollProgress, 2000);
    } else if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [progress, jobs]);

  const handleRunJob = async (jobId: string) => {
    try {
      await api.runBackupJob(jobId);
      setProgress((prev) => ({
        ...prev,
        [jobId]: {
          job_id: jobId,
          app_id: "",
          status: "running",
          percent_done: 0,
          bytes_done: 0,
          bytes_total: 0,
          log_lines: [],
          started_at: new Date().toISOString(),
        },
      }));
    } catch {
      // ignore
    }
  };

  const handleDeleteJob = async (jobId: string) => {
    try {
      await api.deleteBackupJob(jobId);
      setJobs((prev) => prev.filter((j) => j.id !== jobId));
    } catch {
      // ignore
    }
  };

  const handleRunAll = async () => {
    setRunAllState("running");
    try {
      for (const job of jobs) {
        await handleRunJob(job.id);
      }
      setRunAllState("done");
      setTimeout(() => setRunAllState("idle"), 3000);
    } catch {
      setRunAllState("error");
    }
  };

  const handleRestore = async (snapshotId: string, targetPath: string) => {
    setRestoring(snapshotId);
    try {
      await api.backupRestore(snapshotId, targetPath);
    } catch {
      // ignore
    } finally {
      setRestoring(null);
    }
  };

  const appName = (id: string) => {
    const app = apps.find((a) => a.id === id);
    return app?.name || id;
  };

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  // Group jobs by app
  const jobsByApp = new Map<string, BackupJobWithApp[]>();
  for (const job of jobs) {
    const existing = jobsByApp.get(job.app_id) || [];
    existing.push(job);
    jobsByApp.set(job.app_id, existing);
  }

  const runningJobs = jobs.filter((j) => progress[j.id]?.status === "running");
  const displayedSnapshots = snapshots.slice(0, 20);

  return (
    <div class="flex-1 px-6 py-6 max-w-4xl mx-auto w-full space-y-8">
      {/* Header */}
      <div class="flex items-center justify-between">
        <h1 class="text-xl font-bold">Backups</h1>
        <div class="flex gap-2">
          {jobs.length >= 2 && (
            <button
              class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50"
              disabled={runAllState === "running"}
              onClick={handleRunAll}
            >
              {runAllState === "running"
                ? "Running..."
                : runAllState === "done"
                  ? "Done"
                  : "Run All"}
            </button>
          )}
          <button
            class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
            onClick={() => { setEditingJob(null); setShowAddDialog(true); }}
          >
            Add Backup Job
          </button>
        </div>
      </div>

      {/* Running Jobs */}
      {runningJobs.length > 0 && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider mb-3">
            Running
          </h2>
          <div class="space-y-3">
            {runningJobs.map((job) => {
              const p = progress[job.id];
              if (!p) return null;
              return (
                <div key={job.id} class="bg-gray-900 rounded-lg p-4">
                  <div class="flex items-center justify-between mb-2">
                    <div>
                      <span class="text-gray-200 font-medium">{appName(job.app_id)}</span>
                      <span class={`ml-2 text-xs px-1.5 py-0.5 rounded ${destBadge(job).color}`}>
                        {destBadge(job).text}
                      </span>
                    </div>
                    <span class="text-xs text-gray-500">
                      {p.seconds_remaining != null
                        ? `~${Math.ceil(p.seconds_remaining / 60)} min remaining`
                        : ""}
                    </span>
                  </div>
                  {/* Progress bar */}
                  <div class="w-full bg-gray-800 rounded-full h-2 mb-2">
                    <div
                      class="bg-blue-500 h-2 rounded-full transition-all duration-300"
                      style={{ width: `${Math.round(p.percent_done * 100)}%` }}
                    />
                  </div>
                  <div class="flex items-center justify-between text-xs text-gray-500">
                    <span>{Math.round(p.percent_done * 100)}%</span>
                    <span>
                      {formatBytes(p.bytes_done)} / {formatBytes(p.bytes_total)}
                    </span>
                  </div>
                  {p.current_file && (
                    <p class="text-xs text-gray-600 mt-1 font-mono truncate">
                      {p.current_file}
                    </p>
                  )}
                  {/* Expandable log */}
                  {p.log_lines.length > 0 && (
                    <details class="mt-2">
                      <summary class="text-xs text-gray-500 cursor-pointer hover:text-gray-400">
                        Log ({p.log_lines.length} lines)
                      </summary>
                      <pre class="mt-1 text-xs text-gray-600 bg-gray-800 rounded p-2 max-h-40 overflow-y-auto font-mono">
                        {p.log_lines.slice(-50).join("\n")}
                      </pre>
                    </details>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      )}

      {/* Jobs by App */}
      <section>
        <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider mb-3">
          Backup Jobs
        </h2>
        {jobs.length === 0 ? (
          <div class="bg-gray-900 rounded-lg p-6 text-center">
            <p class="text-gray-500 text-sm mb-3">
              No backup jobs configured yet.
            </p>
            <button
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
              onClick={() => { setEditingJob(null); setShowAddDialog(true); }}
            >
              Create your first backup job
            </button>
          </div>
        ) : (
          <div class="space-y-3">
            {Array.from(jobsByApp.entries()).map(([appId, appJobs]) => (
              <div key={appId} class="bg-gray-900 rounded-lg">
                <div class="px-4 py-3 border-b border-gray-800">
                  <button
                    class="text-gray-200 font-medium hover:text-white"
                    onClick={() => route(`/app/${appId}`)}
                  >
                    {appName(appId)}
                  </button>
                </div>
                <div class="divide-y divide-gray-800">
                  {appJobs.map((job) => {
                    const badge = statusBadge(job);
                    const dest = destBadge(job);
                    const isRunning = progress[job.id]?.status === "running";
                    return (
                      <div key={job.id} class="px-4 py-3 flex items-center justify-between gap-3">
                        <div class="min-w-0 flex-1">
                          <div class="flex items-center gap-2 flex-wrap">
                            <span class={`text-xs px-1.5 py-0.5 rounded ${dest.color}`}>
                              {dest.text}
                            </span>
                            <span class="text-xs text-gray-500">
                              {scheduleLabel(job.schedule)}
                            </span>
                            <span class={`text-xs ${badge.color}`}>
                              {badge.text}
                            </span>
                          </div>
                          {job.repository && (
                            <p class="text-xs text-gray-600 font-mono mt-1 truncate">
                              {job.repository}
                            </p>
                          )}
                          {job.last_run_at && (
                            <p class="text-xs text-gray-600 mt-0.5">
                              Last: {formatTimestamp(job.last_run_at)}
                            </p>
                          )}
                          {job.last_error && (
                            <p class="text-xs text-red-400/70 mt-0.5 truncate">
                              {job.last_error}
                            </p>
                          )}
                        </div>
                        <div class="flex gap-2 shrink-0">
                          <button
                            class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50"
                            disabled={isRunning}
                            onClick={() => handleRunJob(job.id)}
                          >
                            {isRunning ? "Running..." : "Run"}
                          </button>
                          <button
                            class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                            onClick={() => { setEditingJob(job); setShowAddDialog(true); }}
                          >
                            Edit
                          </button>
                          <button
                            class="px-2 py-1 bg-red-900/50 hover:bg-red-800/50 text-red-400 text-xs rounded"
                            onClick={() => handleDeleteJob(job.id)}
                          >
                            Delete
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Snapshots */}
      <section>
        <h2 class="text-sm font-medium text-gray-400 mb-4 uppercase tracking-wider">
          Recent Snapshots
        </h2>
        {snapshots.length === 0 ? (
          <p class="text-gray-500 text-sm">
            No snapshots yet. Run a backup to create one.
          </p>
        ) : (
          <div class="space-y-2">
            {displayedSnapshots.map((snap) => (
              <SnapshotRow
                key={snap.id}
                snapshot={snap}
                restoring={restoring === snap.id}
                onRestore={handleRestore}
              />
            ))}
            {snapshots.length > 20 && (
              <p class="text-xs text-gray-500">
                Showing 20 of {snapshots.length} snapshots
              </p>
            )}
          </div>
        )}
      </section>

      {/* Add/Edit Job Dialog */}
      {showAddDialog && (
        <JobDialog
          apps={apps}
          editJob={editingJob}
          onClose={() => { setShowAddDialog(false); setEditingJob(null); }}
          onSaved={() => { setShowAddDialog(false); setEditingJob(null); fetchData(); }}
        />
      )}
    </div>
  );
}

// ── Job Dialog ──────────────────────────────────────────────────────────────

interface JobDialogProps {
  apps: AppInfo[];
  editJob: BackupJobWithApp | null;
  onClose: () => void;
  onSaved: () => void;
}

function JobDialog({ apps, editJob, onClose, onSaved }: JobDialogProps) {
  const isEdit = !!editJob;
  const [appId, setAppId] = useState(editJob?.app_id || (apps[0]?.id ?? ""));
  const [destType, setDestType] = useState(editJob?.destination_type || "remote");
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

  const showCustomCron = isCustomCron(schedule);

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
          app_id: appId,
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
      <div class="bg-gray-900 rounded-lg max-w-lg w-full max-h-[90vh] overflow-y-auto p-6 space-y-4">
        <h2 class="text-lg font-bold text-gray-100">
          {isEdit ? "Edit Backup Job" : "Add Backup Job"}
        </h2>

        {/* App select */}
        {!isEdit && (
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
              onClick={() => setDestType("local")}
            >
              Local
            </button>
            <button
              type="button"
              class={`flex-1 px-3 py-2 text-sm rounded border ${destType === "remote" ? "bg-blue-600 border-blue-500 text-white" : "bg-gray-800 border-gray-700 text-gray-400 hover:border-gray-600"}`}
              onClick={() => setDestType("remote")}
            >
              S3 (Remote)
            </button>
          </div>
          <p class="text-xs text-gray-500 mt-1">
            Leave fields empty to use the default destination from Settings.
          </p>
        </div>

        {/* Destination fields */}
        {destType === "local" ? (
          <div class="space-y-3">
            <div>
              <label class="text-xs text-gray-500 block mb-1">Repository path (optional override)</label>
              <div class="flex gap-2 items-center">
                <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate min-w-0">
                  {repository || "Use default"}
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
            disabled={saving || (!isEdit && !appId)}
            onClick={handleSave}
          >
            {saving ? "Saving..." : isEdit ? "Update" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
