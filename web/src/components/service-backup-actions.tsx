import { useState, useEffect } from "preact/hooks";
import { route } from "preact-router";
import { api, formatTimestamp, type Snapshot } from "../api";
import { PathPicker } from "./path-picker";

interface Props {
  serviceId: string;
}

type RunState = "idle" | "running" | "done" | "error";

function SnapshotItem({
  snapshot,
  restoring,
  onRestore,
}: {
  snapshot: Snapshot;
  restoring: boolean;
  onRestore: (id: string, path: string) => void;
}) {
  const [showRestore, setShowRestore] = useState(false);

  return (
    <div class="bg-gray-800 rounded p-3">
      <div class="flex items-center justify-between">
        <div class="min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <span class="text-gray-300 font-mono text-sm">
              {snapshot.id.slice(0, 8)}
            </span>
            <span class="text-gray-500 text-sm">
              {formatTimestamp(snapshot.time)}
            </span>
          </div>
          {snapshot.tags.length > 0 && (
            <div class="flex gap-1.5 mt-1 flex-wrap">
              {snapshot.tags.map((tag) => (
                <span
                  key={tag}
                  class="text-xs bg-gray-700 text-gray-400 px-1.5 py-0.5 rounded"
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>
        <button
          class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50 shrink-0"
          disabled={restoring}
          onClick={() => setShowRestore(!showRestore)}
        >
          {restoring ? "Restoring..." : showRestore ? "Cancel" : "Restore"}
        </button>
      </div>
      {showRestore && !restoring && (
        <div class="mt-3 border-t border-gray-700 pt-3">
          <p class="text-sm text-gray-400 mb-2">
            Select a directory to restore into:
          </p>
          <PathPicker
            onSelect={(path) => {
              onRestore(snapshot.id, path);
              setShowRestore(false);
            }}
            onCancel={() => setShowRestore(false)}
          />
        </div>
      )}
    </div>
  );
}

export function ServiceBackupActions({ serviceId }: Props) {
  const [runState, setRunState] = useState<RunState>("idle");
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [loadingSnaps, setLoadingSnaps] = useState(true);

  const fetchSnapshots = () => {
    api
      .serviceBackupSnapshots(serviceId)
      .then((snaps) => {
        setSnapshots(snaps);
        setLoadingSnaps(false);
      })
      .catch(() => setLoadingSnaps(false));
  };

  useEffect(() => {
    fetchSnapshots();
  }, [serviceId]);

  const handleBackup = async () => {
    setRunState("running");
    try {
      await api.serviceBackupRun(serviceId);
      setRunState("done");
      fetchSnapshots();
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
              <SnapshotItem
                key={snap.id}
                snapshot={snap}
                restoring={restoring === snap.id}
                onRestore={handleRestore}
              />
            ))}
            {snapshots.length > 10 && (
              <button
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
