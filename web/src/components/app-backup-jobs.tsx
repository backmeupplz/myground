import { useState, useEffect, useRef } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatTimestamp,
  formatBytes,
  formatEta,
  type BackupJobWithApp,
  type BackupJobProgress,
  type RestoreProgress,
  type Snapshot,
  type StorageVolumeStatus,
} from "../api";
import { scheduleLabel, destBadge } from "../utils/backup";
import { isSnapshotDbDump, resolveRestorePath } from "../utils/snapshot";
import { SnapshotRow } from "./snapshot-row";
import { JobDialog } from "./job-dialog";

interface Props {
  appId: string;
  appName: string;
  hasBackupPassword: boolean;
  storage?: StorageVolumeStatus[];
}

/** Info needed to display a run detail popup */
interface RunInfo {
  type: "running" | "last";
  jobId: string;
  status: "running" | "succeeded" | "failed" | "cancelled" | "unknown";
  time: string;
  error?: string;
  logLines?: string[];
}

export function AppBackupJobs({ appId, appName, hasBackupPassword, storage }: Props) {
  const [jobs, setJobs] = useState<BackupJobWithApp[]>([]);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [snapshotsLoading, setSnapshotsLoading] = useState(false);
  const [progress, setProgress] = useState<Record<string, BackupJobProgress>>({});
  const [restoring, setRestoring] = useState<string | null>(null);
  const [restoreProgressMap, setRestoreProgressMap] = useState<Record<string, RestoreProgress>>({});
  const restorePollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [addDestType, setAddDestType] = useState<string | null>(null);
  const [editingJob, setEditingJob] = useState<BackupJobWithApp | null>(null);
  const [detailJob, setDetailJob] = useState<BackupJobWithApp | null>(null);
  const [detailRun, setDetailRun] = useState<RunInfo | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchSnapshots = async () => {
    setSnapshotsLoading(true);
    try {
      const snaps = await api.appBackupSnapshots(appId);
      snaps.sort((a, b) => b.time.localeCompare(a.time));
      setSnapshots(snaps);
    } catch {
      // ignore
    } finally {
      setSnapshotsLoading(false);
    }
  };

  const fetchData = async () => {
    try {
      const allJobs = await api.backupJobs();
      const appJobs = allJobs.filter((j) => j.app_id === appId);
      setJobs(appJobs);

      // Seed progress for jobs the backend reports as running (survives page reload)
      const runningJobs = appJobs.filter((j) => j.last_status === "running");
      for (const j of runningJobs) {
        try {
          const p = await api.backupJobProgress(j.id);
          setProgress((prev) => ({ ...prev, [j.id]: p }));
        } catch {
          // Progress cleared = job finished between status write and now
        }
      }
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
    // Fetch snapshots in background
    fetchSnapshots();
  };

  const pollProgress = async () => {
    const running = jobs.filter((j) => progress[j.id]?.status === "running");
    if (running.length === 0) return;
    for (const j of running) {
      try {
        const p = await api.backupJobProgress(j.id);
        setProgress((prev) => ({ ...prev, [j.id]: p }));
        if (p.status !== "running") {
          fetchData();
        }
      } catch {
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
  }, [appId]);

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

  // Poll restore progress
  const pollRestoreProgress = async () => {
    const running = Object.values(restoreProgressMap).filter((p) => p.status === "running");
    if (running.length === 0) return;
    for (const rp of running) {
      try {
        const p = await api.restoreProgress(rp.restore_id);
        setRestoreProgressMap((prev) => ({ ...prev, [rp.snapshot_id]: p }));
      } catch {
        setRestoreProgressMap((prev) => {
          const next = { ...prev };
          delete next[rp.snapshot_id];
          return next;
        });
        setRestoring(null);
      }
    }
  };

  useEffect(() => {
    const hasRunning = Object.values(restoreProgressMap).some((p) => p.status === "running");
    if (hasRunning) {
      restorePollRef.current = setInterval(pollRestoreProgress, 2000);
    } else if (restorePollRef.current) {
      clearInterval(restorePollRef.current);
      restorePollRef.current = null;
    }
    return () => {
      if (restorePollRef.current) clearInterval(restorePollRef.current);
    };
  }, [restoreProgressMap]);

  const handleRunJob = async (jobId: string) => {
    try {
      await api.runBackupJob(jobId);
      setProgress((prev) => ({
        ...prev,
        [jobId]: {
          job_id: jobId,
          app_id: appId,
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
      if (detailJob?.id === jobId) setDetailJob(null);
    } catch {
      // ignore
    }
  };

  const handleRestore = async (snapshotId: string, targetPath: string) => {
    setRestoring(snapshotId);
    try {
      const res = await api.backupRestore(snapshotId, targetPath);
      if (res.restore_id) {
        setRestoreProgressMap((prev) => ({
          ...prev,
          [snapshotId]: {
            restore_id: res.restore_id,
            snapshot_id: snapshotId,
            app_id: appId,
            status: "running",
            phase: "restoring",
            started_at: new Date().toISOString(),
            log_lines: [],
          },
        }));
      }
    } catch {
      setRestoring(null);
    }
  };

  const handleRestoreDb = async (snapshotId: string) => {
    setRestoring(snapshotId);
    try {
      const res = await api.backupRestoreDb(snapshotId);
      if (res.restore_id) {
        setRestoreProgressMap((prev) => ({
          ...prev,
          [snapshotId]: {
            restore_id: res.restore_id,
            snapshot_id: snapshotId,
            app_id: appId,
            status: "running",
            phase: "extracting",
            started_at: new Date().toISOString(),
            log_lines: [],
          },
        }));
      }
    } catch {
      setRestoring(null);
    }
  };

  const openAdd = (destType?: string) => {
    setEditingJob(null);
    setAddDestType(destType || null);
    setShowAddDialog(true);
  };

  const closeDialog = () => {
    setShowAddDialog(false);
    setEditingJob(null);
    setAddDestType(null);
  };

  const openRunDetail = (job: BackupJobWithApp, type: "running" | "last") => {
    if (type === "running") {
      const p = progress[job.id];
      if (!p) return;
      setDetailRun({
        type: "running",
        jobId: job.id,
        status: "running",
        time: p.started_at,
      });
    } else {
      setDetailRun({
        type: "last",
        jobId: job.id,
        status: job.last_status === "succeeded" ? "succeeded" : job.last_status === "failed" ? "failed" : job.last_status === "cancelled" ? "cancelled" : "unknown",
        time: job.last_run_at || "",
        error: job.last_error,
        logLines: job.last_log_lines,
      });
    }
  };

  if (loading) {
    return <p class="text-gray-500 text-sm">Loading...</p>;
  }

  // No jobs — simple empty state
  if (jobs.length === 0) {
    return (
      <div class="text-center py-2">
        <p class="text-gray-500 text-sm mb-3">No backup jobs configured.</p>
        <button
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
          onClick={() => openAdd()}
        >
          Add Backup Job
        </button>
        {showAddDialog && (
          <JobDialog
            apps={[]}
            editJob={null}
            fixedAppId={appId}
            defaultDestType={addDestType || undefined}
            onClose={closeDialog}
            onSaved={() => { closeDialog(); fetchData(); }}
          />
        )}
      </div>
    );
  }

  const displayedSnapshots = snapshots.slice(0, 5);

  return (
    <div class="space-y-4">
      {/* Job cards with runs underneath */}
      {jobs.map((job) => {
        const dest = destBadge(job);
        const isRunning = progress[job.id]?.status === "running";
        const p = progress[job.id];

        return (
          <div key={job.id}>
            {/* Job header card */}
            <div class="bg-gray-800 rounded-t-lg p-3 border-b border-gray-700">
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-2 min-w-0 flex-wrap">
                  <span class={`text-xs px-1.5 py-0.5 rounded ${dest.color}`}>
                    {dest.text}
                  </span>
                  <span class="text-xs text-gray-400">
                    {scheduleLabel(job.schedule)}
                  </span>
                </div>
                <div class="flex gap-1.5 shrink-0">
                  <button
                    class="px-2 py-1 bg-blue-600 hover:bg-blue-500 text-white text-xs rounded disabled:opacity-50"
                    disabled={isRunning}
                    onClick={() => handleRunJob(job.id)}
                  >
                    Run
                  </button>
                  <button
                    class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                    onClick={() => setDetailJob(job)}
                  >
                    Settings
                  </button>
                </div>
              </div>
            </div>

            {/* Runs list */}
            <div class="bg-gray-800/50 rounded-b-lg divide-y divide-gray-800">
              {/* Current run (in progress) */}
              {isRunning && p && (
                <RunCard
                  status="running"
                  time={p.started_at}
                  progress={p}
                  onViewDetails={() => openRunDetail(job, "running")}
                  onCancel={() => api.cancelBackupJob(job.id)}
                />
              )}

              {/* Skipped indicator */}
              {job.last_skipped_at && (
                <div class="px-3 py-1.5 flex items-center gap-2">
                  <span class="w-2 h-2 rounded-full bg-amber-400 shrink-0" />
                  <span class="text-xs text-amber-400">Skipped {formatTimestamp(job.last_skipped_at)}</span>
                  <span class="text-xs text-gray-600">— previous run still active</span>
                </div>
              )}

              {/* Last run */}
              {job.last_run_at && (
                <RunCard
                  status={job.last_status === "succeeded" ? "succeeded" : job.last_status === "failed" ? "failed" : job.last_status === "cancelled" ? "cancelled" : "unknown"}
                  time={job.last_run_at}
                  error={job.last_error}
                  onViewDetails={() => openRunDetail(job, "last")}
                />
              )}

              {/* No runs yet */}
              {!isRunning && !job.last_run_at && (
                <div class="px-3 py-2">
                  <span class="text-xs text-gray-600">No runs yet</span>
                </div>
              )}
            </div>
          </div>
        );
      })}

      {/* Add another + manage */}
      <div class="flex items-center gap-3 pt-1">
        <button
          class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
          onClick={() => openAdd()}
        >
          + Add Backup Job
        </button>
        <button
          class="text-xs text-gray-500 hover:text-gray-300"
          onClick={() => route("/backups")}
        >
          Manage all backups →
        </button>
      </div>

      {/* Encryption password */}
      {hasBackupPassword && (
        <div class="flex items-center justify-between pt-3 border-t border-gray-800">
          <div class="min-w-0 mr-3">
            <span class="text-gray-300 text-sm">Encryption Password</span>
            <p class="text-xs text-gray-500 font-mono mt-0.5">{"\u2022".repeat(12)}</p>
          </div>
          <CopyPasswordButton appId={appId} />
        </div>
      )}

      {/* Recent snapshots */}
      {(snapshots.length > 0 || snapshotsLoading) && (
        <div class="pt-3 border-t border-gray-800">
          <div class="flex items-center gap-2 mb-2">
            <h3 class="text-sm text-gray-400">Recent Snapshots</h3>
            {snapshotsLoading && (
              <svg class="animate-spin h-3 w-3 text-gray-500" viewBox="0 0 24 24" fill="none">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
            )}
          </div>
          <div class="space-y-2">
            {snapshots.length === 0 && snapshotsLoading && (
              <p class="text-xs text-gray-600">Loading snapshots...</p>
            )}
            {displayedSnapshots.map((snap) => {
              const dbDump = isSnapshotDbDump(snap, appId, storage);
              return (
                <SnapshotRow
                  compact
                  key={snap.id}
                  snapshot={snap}
                  restoring={restoring === snap.id}
                  onRestore={handleRestore}
                  onRestoreDb={handleRestoreDb}
                  defaultRestorePath={resolveRestorePath(snap, appId, storage)}
                  isDbDump={dbDump}
                  restoreProgress={restoreProgressMap[snap.id] || null}
                />
              );
            })}
            {snapshots.length > 5 && (
              <button
                type="button"
                class="text-sm text-blue-400 hover:text-blue-300"
                onClick={() => route("/backups")}
              >
                View all {snapshots.length} snapshots →
              </button>
            )}
          </div>
        </div>
      )}

      {/* Run Detail Dialog */}
      {detailRun && (
        <RunDetailDialog
          run={detailRun}
          progress={progress[detailRun.jobId] || null}
          onClose={() => setDetailRun(null)}
        />
      )}

      {/* Job Settings Dialog */}
      {detailJob && (
        <JobSettingsDialog
          job={detailJob}
          onClose={() => setDetailJob(null)}
          onEdit={() => { setDetailJob(null); setEditingJob(detailJob); setShowAddDialog(true); }}
          onDelete={() => handleDeleteJob(detailJob.id)}
          onRun={() => { handleRunJob(detailJob.id); setDetailJob(null); }}
        />
      )}

      {/* Add/Edit Dialog */}
      {showAddDialog && (
        <JobDialog
          apps={[]}
          editJob={editingJob}
          fixedAppId={appId}
          defaultDestType={addDestType || undefined}
          onClose={closeDialog}
          onSaved={() => { closeDialog(); fetchData(); }}
        />
      )}
    </div>
  );
}

// ── Run Card (compact row) ───────────────────────────────────────────────────

const runStatusConfig = {
  running: { label: "Running", color: "text-blue-400", dot: "bg-blue-400" },
  succeeded: { label: "Succeeded", color: "text-green-400", dot: "bg-green-400" },
  failed: { label: "Failed", color: "text-red-400", dot: "bg-red-400" },
  cancelled: { label: "Cancelled", color: "text-amber-400", dot: "bg-amber-400" },
  unknown: { label: "Unknown", color: "text-gray-400", dot: "bg-gray-400" },
} as const;

function RunCard({
  status,
  time,
  error,
  progress,
  onViewDetails,
  onCancel,
}: {
  status: "running" | "succeeded" | "failed" | "cancelled" | "unknown";
  time: string;
  error?: string;
  progress?: BackupJobProgress;
  onViewDetails: () => void;
  onCancel?: () => void;
}) {
  const cfg = runStatusConfig[status];

  return (
    <div class="px-3 py-2 flex items-center gap-3">
      {status === "running" ? (
        <svg class="animate-spin h-3 w-3 text-blue-400 shrink-0" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
      ) : (
        <span class={`w-2 h-2 rounded-full ${cfg.dot} shrink-0`} />
      )}

      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class={`text-xs font-medium ${cfg.color}`}>{cfg.label}</span>
          <span class="text-xs text-gray-500">{formatTimestamp(time)}</span>
        </div>
        {status === "running" && progress && (
          <div class="flex items-center gap-2 mt-0.5">
            <div class="flex-1 bg-gray-900 rounded-full h-1 max-w-[120px]">
              <div
                class="bg-blue-500 h-1 rounded-full transition-all duration-300"
                style={{ width: `${Math.round(progress.percent_done * 100)}%` }}
              />
            </div>
            <span class="text-xs text-gray-500">{Math.round(progress.percent_done * 100)}%</span>
          </div>
        )}
        {status === "failed" && error && (
          <p class="text-xs text-red-400/70 truncate mt-0.5">
            {error.slice(0, 80)}{error.length > 80 ? "..." : ""}
          </p>
        )}
      </div>

      {status === "running" && onCancel && (
        <button
          class="text-xs text-red-400 hover:text-red-300 shrink-0"
          onClick={onCancel}
        >
          Cancel
        </button>
      )}
      <button
        class="text-xs text-gray-500 hover:text-gray-300 shrink-0"
        onClick={onViewDetails}
      >
        View
      </button>
    </div>
  );
}

// ── Run Detail Dialog ────────────────────────────────────────────────────────

function RunDetailDialog({
  run,
  progress,
  onClose,
}: {
  run: RunInfo;
  progress: BackupJobProgress | null;
  onClose: () => void;
}) {
  const cfg = runStatusConfig[run.status];
  const isRunning = run.status === "running" && progress;

  return (
    <div
      class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        class="bg-gray-900 rounded-xl max-w-lg w-full p-6 max-h-[80vh] overflow-y-auto text-left space-y-4"
        onClick={(e: Event) => e.stopPropagation()}
      >
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-bold text-gray-100">Backup Run</h2>
          <button class="text-gray-500 hover:text-gray-300 text-xl" onClick={onClose}>
            &times;
          </button>
        </div>

        {/* Status + time */}
        <div class="flex items-center gap-2">
          {run.status === "running" ? (
            <svg class="animate-spin h-4 w-4 text-blue-400 shrink-0" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          ) : (
            <span class={`w-2.5 h-2.5 rounded-full ${cfg.dot} shrink-0`} />
          )}
          <span class={`text-sm font-medium ${cfg.color}`}>{cfg.label}</span>
          <span class="text-sm text-gray-400">{formatTimestamp(run.time)}</span>
        </div>

        {/* Running progress */}
        {isRunning && progress && (
          <div class="bg-blue-900/20 border border-blue-500/30 rounded-lg p-3 space-y-2">
            <div class="w-full bg-gray-800 rounded-full h-2">
              <div
                class="bg-blue-500 h-2 rounded-full transition-all duration-300"
                style={{ width: `${Math.round(progress.percent_done * 100)}%` }}
              />
            </div>
            <div class="flex items-center justify-between text-xs text-gray-500">
              <span>{Math.round(progress.percent_done * 100)}%</span>
              <span>{formatBytes(progress.bytes_done)} / {formatBytes(progress.bytes_total)}</span>
            </div>
            {progress.current_file && (
              <p class="text-xs text-gray-600 font-mono truncate">{progress.current_file}</p>
            )}
            {progress.seconds_remaining != null && (
              <p class="text-xs text-gray-500">
                ~{formatEta(progress.seconds_remaining)} remaining
              </p>
            )}
            {progress.log_lines.length > 0 && (
              <div>
                <p class="text-xs text-gray-500 mb-1">Log ({progress.log_lines.length} lines)</p>
                <pre class="text-xs text-gray-600 bg-gray-950 rounded p-2 max-h-60 overflow-y-auto font-mono whitespace-pre-wrap">
                  {progress.log_lines.slice(-100).join("\n")}
                </pre>
              </div>
            )}
          </div>
        )}

        {/* Error (for failed last runs) */}
        {run.error && (
          <div>
            <p class="text-xs text-gray-500 mb-1">Error</p>
            <p class="text-sm text-red-400 break-all">{run.error}</p>
          </div>
        )}

        {/* Log lines (for last runs) */}
        {run.logLines && run.logLines.length > 0 && (
          <div>
            <p class="text-xs text-gray-500 mb-1">Log ({run.logLines.length} lines)</p>
            <pre class="text-xs text-gray-600 bg-gray-950 rounded p-3 max-h-60 overflow-y-auto font-mono whitespace-pre-wrap">
              {run.logLines.join("\n")}
            </pre>
          </div>
        )}

        <div class="pt-2">
          <button
            class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
            onClick={onClose}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Job Settings Dialog ──────────────────────────────────────────────────────

function JobSettingsDialog({
  job,
  onClose,
  onEdit,
  onDelete,
  onRun,
}: {
  job: BackupJobWithApp;
  onClose: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRun: () => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const dest = destBadge(job);

  return (
    <div
      class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        class="bg-gray-900 rounded-xl max-w-lg w-full p-6 max-h-[80vh] overflow-y-auto text-left space-y-4"
        onClick={(e: Event) => e.stopPropagation()}
      >
        <div class="flex items-center justify-between">
          <h2 class="text-lg font-bold text-gray-100">Backup Job Settings</h2>
          <button class="text-gray-500 hover:text-gray-300 text-xl" onClick={onClose}>
            &times;
          </button>
        </div>

        {/* Type & schedule */}
        <div class="space-y-3">
          <div class="flex items-center gap-2">
            <span class={`text-xs px-1.5 py-0.5 rounded ${dest.color}`}>
              {dest.text}
            </span>
            <span class="text-sm text-gray-300">
              {scheduleLabel(job.schedule)}
            </span>
          </div>

          {job.repository && (
            <div>
              <p class="text-xs text-gray-500 mb-0.5">Repository</p>
              <p class="text-sm text-gray-300 font-mono break-all">{job.repository}</p>
            </div>
          )}
        </div>

        {/* Actions */}
        <div class="flex gap-2 pt-2 border-t border-gray-800">
          <button
            class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
            onClick={onRun}
          >
            Run Now
          </button>
          <button
            class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
            onClick={onEdit}
          >
            Edit
          </button>
          {!confirmDelete ? (
            <button
              class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-red-400 text-sm rounded ml-auto"
              onClick={() => setConfirmDelete(true)}
            >
              Delete
            </button>
          ) : (
            <div class="ml-auto space-y-2">
              <p class="text-xs text-red-300">All snapshots for this job will be deleted.</p>
              <div class="flex gap-2">
                <button
                  class="px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white text-sm rounded"
                  onClick={onDelete}
                >
                  Confirm Delete
                </button>
                <button
                  class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
                  onClick={() => setConfirmDelete(false)}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Copy Password Button ─────────────────────────────────────────────────────

function CopyPasswordButton({ appId }: { appId: string }) {
  const [state, setState] = useState<"idle" | "loading" | "copied" | "error">("idle");
  return (
    <button
      type="button"
      class="text-xs text-blue-400 hover:text-blue-300 shrink-0 disabled:opacity-50"
      disabled={state === "loading"}
      onClick={async () => {
        setState("loading");
        try {
          const res = await api.getBackupPassword(appId);
          if (res.password) {
            await navigator.clipboard.writeText(res.password);
            setState("copied");
            setTimeout(() => setState("idle"), 2000);
          } else {
            setState("error");
            setTimeout(() => setState("idle"), 2000);
          }
        } catch {
          setState("error");
          setTimeout(() => setState("idle"), 2000);
        }
      }}
    >
      {state === "loading" ? "..." : state === "copied" ? "Copied!" : state === "error" ? "Error" : "Copy"}
    </button>
  );
}
