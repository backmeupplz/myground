import { useState, useEffect, useRef } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatTimestamp,
  formatBytes,
  type AppInfo,
  type BackupJobWithApp,
  type BackupJobProgress,
  type RestoreProgress,
  type Snapshot,
} from "../api";
import { SnapshotRow } from "../components/snapshot-row";
import { JobDialog } from "../components/job-dialog";
import { scheduleLabel, statusBadge, destBadge } from "../utils/backup";
import { isSnapshotDbDump, resolveRestorePath } from "../utils/snapshot";

interface Props {
  path?: string;
}

export function Backups({}: Props) {
  const [jobs, setJobs] = useState<BackupJobWithApp[]>([]);
  const [apps, setApps] = useState<AppInfo[]>([]);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [snapshotsLoading, setSnapshotsLoading] = useState(false);
  const [progress, setProgress] = useState<Record<string, BackupJobProgress>>({});
  const [restoring, setRestoring] = useState<string | null>(null);
  const [restoreProgressMap, setRestoreProgressMap] = useState<Record<string, RestoreProgress>>({});
  const restorePollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [editingJob, setEditingJob] = useState<BackupJobWithApp | null>(null);
  const [runAllState, setRunAllState] = useState<"idle" | "running" | "done" | "error">("idle");
  const [confirmingDeleteId, setConfirmingDeleteId] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchSnapshots = async (backupApps: AppInfo[]) => {
    setSnapshotsLoading(true);
    try {
      const seen = new Set<string>();
      const allSnaps: Snapshot[] = [];
      // Fetch per-app, streaming results as each resolves
      for (const app of backupApps) {
        try {
          const snaps = await api.appBackupSnapshots(app.id);
          for (const s of snaps) {
            if (!seen.has(s.id)) {
              seen.add(s.id);
              allSnaps.push(s);
            }
          }
          // Update after each app so local results appear immediately
          const sorted = [...allSnaps].sort((a, b) => b.time.localeCompare(a.time));
          setSnapshots(sorted);
        } catch {
          // continue
        }
      }
    } finally {
      setSnapshotsLoading(false);
    }
  };

  const fetchData = async () => {
    try {
      const [jobList, appList] = await Promise.all([
        api.backupJobs(),
        api.apps(),
      ]);
      setJobs(jobList);
      const backupApps = appList.filter((a) => a.installed && a.backup_supported);
      setApps(backupApps);
      setLoading(false);

      // Seed progress for jobs the backend reports as running (survives page reload)
      const runningJobs = jobList.filter((j) => j.last_status === "running");
      for (const j of runningJobs) {
        try {
          const p = await api.backupJobProgress(j.id);
          setProgress((prev) => ({ ...prev, [j.id]: p }));
        } catch {
          // Progress cleared = job finished between status write and now
        }
      }

      // Fetch snapshots in background (doesn't block page render)
      fetchSnapshots(backupApps);
    } catch {
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
      const res = await api.backupRestore(snapshotId, targetPath);
      if (res.restore_id) {
        setRestoreProgressMap((prev) => ({
          ...prev,
          [snapshotId]: {
            restore_id: res.restore_id,
            snapshot_id: snapshotId,
            app_id: "",
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

  const appName = (id: string) => {
    const app = apps.find((a) => a.id === id);
    return app?.name || id;
  };

  /** Resolve snapshot tags to the original host path for restore pre-fill. */
  const resolveSnapRestorePath = (snap: Snapshot): string | undefined => {
    for (const app of apps) {
      const path = resolveRestorePath(snap, app.id, app.storage);
      if (path) return path;
    }
    return undefined;
  };

  /** Check if a snapshot is a database dump. */
  const isSnapDbDump = (snap: Snapshot): boolean => {
    for (const app of apps) {
      if (isSnapshotDbDump(snap, app.id, app.storage)) return true;
    }
    return false;
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
            app_id: "",
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
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full space-y-6 sm:space-y-8">
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

      {/* Running Restores */}
      {Object.values(restoreProgressMap).some((p) => p.status === "running") && (
        <section>
          <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider mb-3">
            Restoring
          </h2>
          <div class="space-y-3">
            {Object.values(restoreProgressMap)
              .filter((p) => p.status === "running")
              .map((rp) => (
                <div key={rp.restore_id} class="bg-gray-900 rounded-lg p-4">
                  <div class="flex items-center gap-2 mb-2">
                    <svg class="animate-spin h-4 w-4 text-blue-400 shrink-0" viewBox="0 0 24 24" fill="none">
                      <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                      <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                    </svg>
                    <span class="text-gray-200 font-medium">Restoring snapshot {rp.snapshot_id.slice(0, 8)}</span>
                    <span class="text-xs text-blue-400 capitalize">{rp.phase}</span>
                  </div>
                  {rp.log_lines.length > 0 && (
                    <p class="text-xs text-gray-500 truncate">{rp.log_lines[rp.log_lines.length - 1]}</p>
                  )}
                </div>
              ))}
          </div>
        </section>
      )}

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
                      <div key={job.id} class="px-4 py-3 flex items-start justify-between gap-3">
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
                          {job.last_log_lines && job.last_log_lines.length > 0 && (
                            <details class="mt-1">
                              <summary class="text-xs text-gray-500 cursor-pointer hover:text-gray-400">
                                Last run log ({job.last_log_lines.length} lines)
                              </summary>
                              <pre class="mt-1 text-xs text-gray-600 bg-gray-800 rounded p-2 max-h-40 overflow-y-auto font-mono whitespace-pre-wrap">
                                {job.last_log_lines.join("\n")}
                              </pre>
                            </details>
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
                          {confirmingDeleteId === job.id ? (
                            <div class="flex gap-1">
                              <button
                                class="px-2 py-1 bg-red-600 hover:bg-red-500 text-white text-xs rounded"
                                onClick={() => { handleDeleteJob(job.id); setConfirmingDeleteId(null); }}
                              >
                                Confirm
                              </button>
                              <button
                                class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                                onClick={() => setConfirmingDeleteId(null)}
                              >
                                Cancel
                              </button>
                            </div>
                          ) : (
                            <button
                              class="px-2 py-1 bg-red-900/50 hover:bg-red-800/50 text-red-400 text-xs rounded"
                              onClick={() => setConfirmingDeleteId(job.id)}
                            >
                              Delete
                            </button>
                          )}
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
        <div class="flex items-center gap-2 mb-4">
          <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
            Recent Snapshots
          </h2>
          {snapshotsLoading && (
            <svg class="animate-spin h-3.5 w-3.5 text-gray-500" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          )}
        </div>
        {snapshots.length === 0 && !snapshotsLoading ? (
          <p class="text-gray-500 text-sm">
            No snapshots yet. Run a backup to create one.
          </p>
        ) : snapshots.length === 0 && snapshotsLoading ? (
          <p class="text-gray-500 text-sm">Loading snapshots...</p>
        ) : (
          <div class="space-y-2">
            {displayedSnapshots.map((snap) => (
              <SnapshotRow
                key={snap.id}
                snapshot={snap}
                restoring={restoring === snap.id}
                onRestore={handleRestore}
                onRestoreDb={handleRestoreDb}
                defaultRestorePath={resolveSnapRestorePath(snap)}
                isDbDump={isSnapDbDump(snap)}
                restoreProgress={restoreProgressMap[snap.id] || null}
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

