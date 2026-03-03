import { useState, useEffect } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  type AppInfo,
  type AppBackupConfig,
  type Snapshot,
} from "../api";
import { SnapshotRow } from "../components/snapshot-row";

interface AppBackupStatus {
  app: AppInfo;
  config: AppBackupConfig | null;
}

type RunState = "idle" | "running" | "done" | "error";

interface Props {
  path?: string;
}

function backupStatusLabel(cfg: AppBackupConfig | null): string {
  if (!cfg || !cfg.enabled) return "Not configured";
  const hasLocal = !!cfg.local?.repository;
  const hasRemote = !!cfg.remote?.repository;
  if (hasLocal && hasRemote) return "Configured (Local + S3)";
  if (hasLocal) return "Configured (Local)";
  if (hasRemote) return "Configured (S3)";
  return "Enabled (no repos set)";
}

function backupStatusColor(cfg: AppBackupConfig | null): string {
  if (!cfg || !cfg.enabled) return "text-gray-500";
  const hasLocal = !!cfg.local?.repository;
  const hasRemote = !!cfg.remote?.repository;
  if (hasLocal || hasRemote) return "text-green-400";
  return "text-yellow-400";
}

function isConfigured(cfg: AppBackupConfig | null): boolean {
  if (!cfg || !cfg.enabled) return false;
  return !!(cfg.local?.repository || cfg.remote?.repository);
}

export function Backups({}: Props) {
  const [statuses, setStatuses] = useState<AppBackupStatus[]>([]);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [runStates, setRunStates] = useState<Record<string, RunState>>({});
  const [runAllState, setRunAllState] = useState<RunState>("idle");
  const [restoring, setRestoring] = useState<string | null>(null);

  const fetchData = async () => {
    try {
      const allApps = await api.apps();
      const backupApps = allApps.filter(
        (s) => s.installed && s.backup_supported,
      );

      const configs = await Promise.all(
        backupApps.map((s) =>
          api.getAppBackup(s.id).catch(() => null),
        ),
      );

      setStatuses(
        backupApps.map((s, i) => ({ app: s, config: configs[i] })),
      );

      // Fetch snapshots from all apps
      const snapshotResults = await Promise.all(
        backupApps.map((s) =>
          api.appBackupSnapshots(s.id).catch(() => [] as Snapshot[]),
        ),
      );

      // Deduplicate by snapshot ID and sort by time desc
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

  useEffect(() => {
    fetchData();
  }, []);

  const handleBackupApp = async (id: string) => {
    setRunStates((prev) => ({ ...prev, [id]: "running" }));
    try {
      await api.appBackupRun(id);
      setRunStates((prev) => ({ ...prev, [id]: "done" }));
      fetchData();
    } catch {
      setRunStates((prev) => ({ ...prev, [id]: "error" }));
    }
  };

  const handleBackupAll = async () => {
    setRunAllState("running");
    try {
      await api.backupRunAll();
      setRunAllState("done");
      fetchData();
    } catch {
      setRunAllState("error");
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

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  const displayedSnapshots = snapshots.slice(0, 20);

  return (
    <div class="flex-1 px-6 py-6 max-w-4xl mx-auto w-full space-y-8">
      <h1 class="text-xl font-bold">Backups</h1>
      {/* Section 1: Apps */}
      <section>
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
            Apps
          </h2>
          {statuses.filter(({ config }) => isConfigured(config)).length >= 2 && (
            <button
              class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
              disabled={runAllState === "running"}
              onClick={handleBackupAll}
            >
              {runAllState === "running"
                ? "Backing up..."
                : runAllState === "done"
                  ? "Done"
                  : runAllState === "error"
                    ? "Error"
                    : "Back Up All Apps"}
            </button>
          )}
        </div>

        {statuses.length === 0 ? (
          <p class="text-gray-500 text-sm">
            No installed apps with backup support.
          </p>
        ) : (
          <div class="grid gap-3">
            {statuses.map(({ app, config }) => {
              const rs = runStates[app.id] || "idle";
              return (
                <div
                  key={app.id}
                  class="bg-gray-900 rounded-lg p-4 flex items-center justify-between"
                >
                  <div class="min-w-0">
                    <button
                      class="text-gray-200 font-medium hover:text-white block"
                      onClick={() => route(`/app/${app.id}`)}
                    >
                      {app.name}
                    </button>
                    {isConfigured(config) ? (
                      <p
                        class={`text-sm mt-1 ${backupStatusColor(config)}`}
                      >
                        {backupStatusLabel(config)}
                      </p>
                    ) : (
                      <button
                        class="text-sm mt-1 text-gray-500 hover:text-blue-400 block"
                        onClick={() => route(`/app/${app.id}`)}
                      >
                        Not configured — click to set up
                      </button>
                    )}
                    {config?.local?.repository && (
                      <p class="text-xs text-gray-600 font-mono mt-0.5">
                        Local: {config.local.repository}
                      </p>
                    )}
                    {config?.remote?.repository && (
                      <p class="text-xs text-gray-600 font-mono mt-0.5">
                        S3: {config.remote.repository}
                      </p>
                    )}
                  </div>
                  {isConfigured(config) ? (
                    <button
                      class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50 shrink-0"
                      disabled={rs === "running"}
                      onClick={() => handleBackupApp(app.id)}
                    >
                      {rs === "running"
                        ? "Running..."
                        : rs === "done"
                          ? "Done"
                          : rs === "error"
                            ? "Error"
                            : "Back Up Now"}
                    </button>
                  ) : (
                    <button
                      class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded shrink-0"
                      onClick={() => route(`/app/${app.id}`)}
                    >
                      Configure
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* Section 2: Recent Snapshots */}
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

    </div>
  );
}

