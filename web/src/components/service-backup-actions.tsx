import { useEffect, useState } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  type Snapshot,
  type ServiceBackupConfig,
} from "../api";
import { SnapshotRow } from "./snapshot-row";

interface Props {
  serviceId: string;
}

type RunState = "idle" | "running" | "done" | "error";

function hasConfiguredRepo(cfg: ServiceBackupConfig | null): boolean {
  if (!cfg) return false;
  if (!cfg.enabled && !cfg.remote) return false;
  return !!(cfg.local?.repository || cfg.remote?.repository);
}

export function ServiceBackupActions({ serviceId }: Props) {
  const [runState, setRunState] = useState<RunState>("idle");
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [loadingSnaps, setLoadingSnaps] = useState(true);
  const [backupConfig, setBackupConfig] =
    useState<ServiceBackupConfig | null>(null);

  const fetchData = () => {
    api
      .getServiceBackup(serviceId)
      .then(setBackupConfig)
      .catch(() => {});
    api
      .serviceBackupSnapshots(serviceId)
      .then((snaps) => {
        setSnapshots(snaps);
        setLoadingSnaps(false);
      })
      .catch(() => setLoadingSnaps(false));
  };

  useEffect(() => {
    fetchData();
  }, [serviceId]);

  // Don't render anything when backups aren't configured
  if (!hasConfiguredRepo(backupConfig)) return null;

  const handleBackup = async () => {
    setRunState("running");
    try {
      await api.serviceBackupRun(serviceId);
      setRunState("done");
      fetchData();
    } catch {
      setRunState("error");
    }
  };

  const handleRestore = async (snapshotId: string, targetPath: string) => {
    setRestoring(snapshotId);
    try {
      await api.backupRestore(snapshotId, targetPath);
      setRestoring(null);
    } catch {
      setRestoring(null);
    }
  };

  const displayed = snapshots.slice(0, 10);

  return (
    <div class="space-y-4 mt-4">
      {/* Back Up Now */}
      <div class="flex items-center gap-3">
        <button
          type="button"
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
          disabled={runState === "running"}
          onClick={handleBackup}
        >
          {runState === "running"
            ? "Backing up..."
            : runState === "done"
              ? "Backup complete"
              : runState === "error"
                ? "Backup failed"
                : "Back Up Now"}
        </button>
        {runState === "done" && (
          <span class="text-sm text-green-400">Done</span>
        )}
        {runState === "error" && (
          <span class="text-sm text-red-400">Error</span>
        )}
      </div>

      {/* Recent Snapshots */}
      <div>
        <h3 class="text-sm text-gray-400 mb-2">Recent Snapshots</h3>
        {loadingSnaps ? (
          <p class="text-gray-500 text-sm">Loading...</p>
        ) : snapshots.length === 0 ? (
          <p class="text-gray-500 text-sm">
            No snapshots yet. Run a backup to create one.
          </p>
        ) : (
          <div class="space-y-2">
            {displayed.map((snap) => (
              <SnapshotRow
                compact
                key={snap.id}
                snapshot={snap}
                restoring={restoring === snap.id}
                onRestore={handleRestore}
              />
            ))}
            {snapshots.length > 10 && (
              <button
                type="button"
                class="text-sm text-blue-400 hover:text-blue-300"
                onClick={() => route("/backups")}
              >
                View all {snapshots.length} snapshots
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
