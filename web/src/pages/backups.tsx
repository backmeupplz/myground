import { useState, useEffect } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatTimestamp,
  type ServiceInfo,
  type ServiceBackupConfig,
  type Snapshot,
} from "../api";
import { PathPicker } from "../components/path-picker";

interface ServiceBackupStatus {
  service: ServiceInfo;
  config: ServiceBackupConfig | null;
}

type RunState = "idle" | "running" | "done" | "error";

interface Props {
  path?: string;
}

function backupStatusLabel(cfg: ServiceBackupConfig | null): string {
  if (!cfg || !cfg.enabled) return "Not configured";
  const hasLocal = !!cfg.local?.repository;
  const hasRemote = !!cfg.remote?.repository;
  if (hasLocal && hasRemote) return "Configured (Local + S3)";
  if (hasLocal) return "Configured (Local)";
  if (hasRemote) return "Configured (S3)";
  return "Enabled (no repos set)";
}

function backupStatusColor(cfg: ServiceBackupConfig | null): string {
  if (!cfg || !cfg.enabled) return "text-gray-500";
  const hasLocal = !!cfg.local?.repository;
  const hasRemote = !!cfg.remote?.repository;
  if (hasLocal || hasRemote) return "text-green-400";
  return "text-yellow-400";
}

function isConfigured(cfg: ServiceBackupConfig | null): boolean {
  if (!cfg || !cfg.enabled) return false;
  return !!(cfg.local?.repository || cfg.remote?.repository);
}

export function Backups({}: Props) {
  const [statuses, setStatuses] = useState<ServiceBackupStatus[]>([]);
  const [snapshots, setSnapshots] = useState<Snapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [runStates, setRunStates] = useState<Record<string, RunState>>({});
  const [runAllState, setRunAllState] = useState<RunState>("idle");
  const [restoring, setRestoring] = useState<string | null>(null);

  const fetchData = async () => {
    try {
      const services = await api.services();
      const backupServices = services.filter(
        (s) => s.installed && s.backup_supported,
      );

      const configs = await Promise.all(
        backupServices.map((s) =>
          api.getServiceBackup(s.id).catch(() => null),
        ),
      );

      setStatuses(
        backupServices.map((s, i) => ({ service: s, config: configs[i] })),
      );

      // Fetch snapshots from all services
      const snapshotResults = await Promise.all(
        backupServices.map((s) =>
          api.serviceBackupSnapshots(s.id).catch(() => [] as Snapshot[]),
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

  const handleBackupService = async (id: string) => {
    setRunStates((prev) => ({ ...prev, [id]: "running" }));
    try {
      await api.serviceBackupRun(id);
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
      {/* Section 1: Services */}
      <section>
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
            Services
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
                    : "Back Up All Services"}
            </button>
          )}
        </div>

        {statuses.length === 0 ? (
          <p class="text-gray-500 text-sm">
            No installed services with backup support.
          </p>
        ) : (
          <div class="grid gap-3">
            {statuses.map(({ service, config }) => {
              const rs = runStates[service.id] || "idle";
              return (
                <div
                  key={service.id}
                  class="bg-gray-900 rounded-lg p-4 flex items-center justify-between"
                >
                  <div class="min-w-0">
                    <button
                      class="text-gray-200 font-medium hover:text-white block"
                      onClick={() => route(`/service/${service.id}`)}
                    >
                      {service.name}
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
                        onClick={() => route(`/service/${service.id}`)}
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
                      onClick={() => handleBackupService(service.id)}
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
                      onClick={() => route(`/service/${service.id}`)}
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

function SnapshotRow({
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
    <div class="bg-gray-900 rounded-lg p-3">
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
                  class="text-xs bg-gray-800 text-gray-400 px-1.5 py-0.5 rounded"
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>
        <button
          class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50 shrink-0"
          disabled={restoring}
          onClick={() => setShowRestore(!showRestore)}
        >
          {restoring ? "Restoring..." : showRestore ? "Cancel" : "Restore"}
        </button>
      </div>
      {showRestore && !restoring && (
        <div class="mt-3 border-t border-gray-800 pt-3">
          <p class="text-sm text-gray-400 mb-2">
            Select a directory to restore this snapshot into:
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
