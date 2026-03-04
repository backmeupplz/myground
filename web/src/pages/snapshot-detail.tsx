import { useState, useEffect, useRef } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatTimestamp,
  formatBytes,
  type AppInfo,
  type Snapshot,
  type SnapshotFile,
  type RestoreProgress,
} from "../api";
import { PathPicker } from "../components/path-picker";

interface Props {
  id?: string;
  path?: string;
}

export function SnapshotDetail({ id }: Props) {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [files, setFiles] = useState<SnapshotFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [filesLoading, setFilesLoading] = useState(true);
  const [currentPath, setCurrentPath] = useState<string | undefined>(undefined);
  const [showRestore, setShowRestore] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState("");
  const [defaultRestorePath, setDefaultRestorePath] = useState<string | undefined>(undefined);
  const [isDbDump, setIsDbDump] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [confirmText, setConfirmText] = useState("");
  const [restoreProgressData, setRestoreProgressData] = useState<RestoreProgress | null>(null);
  const restorePollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Load snapshot metadata by finding it in app snapshots
  useEffect(() => {
    if (!id) return;
    (async () => {
      try {
        const apps = await api.apps();
        const backupApps = apps.filter((a) => a.installed && a.backup_supported);
        for (const app of backupApps) {
          try {
            const snaps = await api.appBackupSnapshots(app.id);
            const found = snaps.find((s) => s.id === id || s.id.startsWith(id!));
            if (found) {
              setSnapshot(found);
              // Resolve default restore path from snapshot tags + app storage
              for (const tag of found.tags) {
                const slashIdx = tag.indexOf("/");
                if (slashIdx < 0) continue;
                const volName = tag.slice(slashIdx + 1);
                const vol = app.storage.find((s) => s.name === volName);
                if (vol) {
                  setDefaultRestorePath(vol.host_path);
                  if (vol.is_db_dump) setIsDbDump(true);
                  break;
                }
              }
              break;
            }
          } catch {
            // continue
          }
        }
      } catch {
        // ignore
      } finally {
        setLoading(false);
      }
    })();
  }, [id]);

  // Load files
  useEffect(() => {
    if (!id) return;
    setFilesLoading(true);
    api
      .snapshotFiles(id, currentPath)
      .then(setFiles)
      .catch(() => setFiles([]))
      .finally(() => setFilesLoading(false));
  }, [id, currentPath]);

  // Poll restore progress
  useEffect(() => {
    if (!restoreProgressData || restoreProgressData.status !== "running") {
      if (restorePollRef.current) {
        clearInterval(restorePollRef.current);
        restorePollRef.current = null;
      }
      return;
    }
    restorePollRef.current = setInterval(async () => {
      try {
        const p = await api.restoreProgress(restoreProgressData.restore_id);
        setRestoreProgressData(p);
        if (p.status !== "running") {
          setRestoring(false);
        }
      } catch {
        setRestoreProgressData(null);
        setRestoring(false);
      }
    }, 2000);
    return () => {
      if (restorePollRef.current) clearInterval(restorePollRef.current);
    };
  }, [restoreProgressData?.restore_id, restoreProgressData?.status]);

  const initialPath = defaultRestorePath || "/";
  const effectivePath = selectedPath ?? initialPath;
  const isOriginalPath = defaultRestorePath && (() => {
    const norm = (p: string) => p.replace(/\/+$/, "");
    const a = norm(effectivePath);
    const b = norm(defaultRestorePath);
    if (a === b) return true;
    if (b.startsWith("~")) return a.endsWith(b.slice(1));
    return false;
  })();

  const handleRestore = async () => {
    if (!id) return;
    setRestoring(true);
    setError("");
    try {
      const res = isDbDump
        ? await api.backupRestoreDb(id)
        : await api.backupRestore(id, effectivePath);
      setShowRestore(false);
      setSelectedPath(null);
      setConfirmText("");
      if (res.restore_id) {
        setRestoreProgressData({
          restore_id: res.restore_id,
          snapshot_id: id,
          app_id: "",
          status: "running",
          phase: isDbDump ? "extracting" : "restoring",
          started_at: new Date().toISOString(),
          log_lines: [],
        });
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Restore failed");
      setRestoring(false);
    }
  };

  const handleDelete = async () => {
    if (!id) return;
    setDeleting(true);
    try {
      await api.deleteSnapshot(id);
      route("/backups");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Delete failed");
      setDeleting(false);
    }
  };

  // Build breadcrumb segments from currentPath
  const breadcrumbs: { label: string; path: string | undefined }[] = [
    { label: "/", path: undefined },
  ];
  if (currentPath) {
    const parts = currentPath.replace(/^\//, "").split("/").filter(Boolean);
    let accumulated = "";
    for (const part of parts) {
      accumulated += "/" + part;
      breadcrumbs.push({ label: part, path: accumulated });
    }
  }

  // Filter files to show only immediate children of currentPath (directory listing)
  const prefix = currentPath || "";
  const displayFiles = files.filter((f) => {
    if (!prefix) {
      // Show top-level entries (depth 1 from root)
      const segments = f.path.replace(/^\//, "").split("/").filter(Boolean);
      return segments.length === 1;
    }
    if (!f.path.startsWith(prefix + "/")) return false;
    const rest = f.path.slice(prefix.length + 1);
    return rest.length > 0 && !rest.includes("/");
  });

  // If no immediate children match, show all files under the prefix
  const showFiles = displayFiles.length > 0 ? displayFiles : files;

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  return (
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full space-y-6">
      {/* Back link */}
      <button
        class="text-sm text-gray-500 hover:text-gray-300"
        onClick={() => route("/backups")}
      >
        &larr; Back to Backups
      </button>

      {/* Header */}
      <div>
        <h1 class="text-xl font-bold font-mono break-all">
          {id}
        </h1>
        {snapshot && (
          <div class="mt-2 space-y-1">
            <p class="text-sm text-gray-400">
              {formatTimestamp(snapshot.time)}
              {snapshot.hostname && (
                <span class="text-gray-600 ml-2">on {snapshot.hostname}</span>
              )}
            </p>
            {(snapshot.tags.length > 0 || isDbDump) && (
              <div class="flex gap-1.5 flex-wrap">
                {isDbDump && (
                  <span class="text-xs px-1.5 py-0.5 rounded bg-purple-900/50 text-purple-400">
                    Database Dump
                  </span>
                )}
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
            {snapshot.paths.length > 0 && (
              <p class="text-xs text-gray-600 font-mono">
                {snapshot.paths.join(", ")}
              </p>
            )}
          </div>
        )}
      </div>

      {/* Actions */}
      <div class="flex gap-2">
        <button
          class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50"
          disabled={restoring}
          onClick={() => { setShowRestore(!showRestore); setSelectedPath(null); setConfirmText(""); }}
        >
          {restoring ? "Restoring..." : showRestore ? "Cancel Restore" : isDbDump ? "Restore to DB" : "Restore"}
        </button>
        {!confirmDelete ? (
          <button
            class="px-3 py-1.5 bg-red-900/50 hover:bg-red-800/50 text-red-400 text-sm rounded disabled:opacity-50"
            disabled={deleting}
            onClick={() => setConfirmDelete(true)}
          >
            Delete
          </button>
        ) : (
          <div class="flex gap-2">
            <button
              class="px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50"
              disabled={deleting}
              onClick={handleDelete}
            >
              {deleting ? "Deleting..." : "Confirm Delete"}
            </button>
            <button
              class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
              onClick={() => setConfirmDelete(false)}
            >
              Cancel
            </button>
          </div>
        )}
      </div>

      {error && (
        <p class="text-red-400 text-sm">{error}</p>
      )}

      {/* Restore progress */}
      {restoreProgressData && restoreProgressData.status === "running" && (
        <div class="bg-blue-900/20 border border-blue-500/30 rounded-lg p-4 space-y-2">
          <div class="flex items-center gap-2">
            <svg class="animate-spin h-4 w-4 text-blue-400 shrink-0" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
            <span class="text-sm text-blue-400 font-medium capitalize">{restoreProgressData.phase}...</span>
          </div>
          {restoreProgressData.log_lines.length > 0 && (
            <p class="text-xs text-gray-500">{restoreProgressData.log_lines[restoreProgressData.log_lines.length - 1]}</p>
          )}
        </div>
      )}
      {restoreProgressData && restoreProgressData.status === "succeeded" && (
        <div class="bg-green-900/20 border border-green-500/30 rounded-lg p-4">
          <span class="text-sm text-green-400">Restore completed successfully</span>
        </div>
      )}
      {restoreProgressData && restoreProgressData.status === "failed" && (
        <div class="bg-red-900/20 border border-red-500/30 rounded-lg p-4">
          <span class="text-sm text-red-400">Restore failed: {restoreProgressData.error || "Unknown error"}</span>
        </div>
      )}

      {/* Restore: database dump */}
      {showRestore && !restoring && isDbDump && (
        <div class="bg-gray-900 rounded-lg p-4 space-y-3">
          <div class="bg-red-900/20 border border-red-500/30 rounded-lg p-3 space-y-2">
            <p class="text-sm text-red-300">
              This will wipe the current database and replace it with the backup. All existing data will be lost.
            </p>
            <p class="text-xs text-gray-400">
              Type <span class="font-mono text-red-400">restore</span> to confirm.
            </p>
            <div class="flex gap-2 items-center">
              <input
                type="text"
                value={confirmText}
                onInput={(e) => setConfirmText((e.target as HTMLInputElement).value)}
                class="bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono w-32"
                placeholder="restore"
              />
              <button
                type="button"
                class="px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50"
                disabled={confirmText !== "restore"}
                onClick={handleRestore}
              >
                Wipe & Restore
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Restore: file path picker */}
      {showRestore && !restoring && !isDbDump && (
        <div class="bg-gray-900 rounded-lg p-4 space-y-3">
          {defaultRestorePath && (
            <p class="text-xs text-gray-500">
              Original location: <span class="font-mono text-gray-400">{defaultRestorePath}</span>
            </p>
          )}

          <PathPicker
            initialPath={initialPath}
            onSelect={(path) => setSelectedPath(path)}
            onCancel={() => { setShowRestore(false); setSelectedPath(null); setConfirmText(""); }}
          />

          {selectedPath && isOriginalPath && (
            <div class="bg-red-900/20 border border-red-500/30 rounded-lg p-3 space-y-2">
              <p class="text-sm text-red-300">
                This will overwrite existing data in <span class="font-mono">{defaultRestorePath}</span>
              </p>
              <p class="text-xs text-gray-400">
                Type <span class="font-mono text-red-400">restore</span> to confirm.
              </p>
              <div class="flex gap-2 items-center">
                <input
                  type="text"
                  value={confirmText}
                  onInput={(e) => setConfirmText((e.target as HTMLInputElement).value)}
                  class="bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono w-32"
                  placeholder="restore"
                />
                <button
                  type="button"
                  class="px-3 py-1.5 bg-red-600 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50"
                  disabled={confirmText !== "restore"}
                  onClick={handleRestore}
                >
                  Restore
                </button>
              </div>
            </div>
          )}

          {selectedPath && !isOriginalPath && (
            <button
              type="button"
              class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
              onClick={handleRestore}
            >
              Restore to {selectedPath}
            </button>
          )}
        </div>
      )}

      {/* File browser */}
      <section>
        <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
          Files
        </h2>

        {/* Breadcrumb */}
        <div class="flex items-center gap-1 mb-3 text-sm flex-wrap">
          {breadcrumbs.map((bc, i) => (
            <span key={i} class="flex items-center gap-1">
              {i > 0 && <span class="text-gray-600">/</span>}
              <button
                class={`hover:text-gray-200 ${
                  i === breadcrumbs.length - 1
                    ? "text-gray-300"
                    : "text-gray-500"
                }`}
                onClick={() => setCurrentPath(bc.path)}
              >
                {bc.label}
              </button>
            </span>
          ))}
        </div>

        {filesLoading ? (
          <p class="text-gray-500 text-sm">Loading files...</p>
        ) : showFiles.length === 0 ? (
          <p class="text-gray-500 text-sm">No files found.</p>
        ) : (
          <div class="bg-gray-900 rounded-lg overflow-hidden">
            <table class="w-full text-sm">
              <thead>
                <tr class="text-gray-500 text-xs border-b border-gray-800">
                  <th class="text-left px-4 py-2 font-medium">Name</th>
                  <th class="text-right px-4 py-2 font-medium">Size</th>
                  <th class="text-right px-4 py-2 font-medium hidden sm:table-cell">
                    Modified
                  </th>
                </tr>
              </thead>
              <tbody>
                {showFiles.map((f) => {
                  const name = f.path.split("/").pop() || f.path;
                  const isDir = f.type === "dir";
                  return (
                    <tr
                      key={f.path}
                      class={`border-b border-gray-800/50 ${
                        isDir
                          ? "cursor-pointer hover:bg-gray-800/50"
                          : ""
                      }`}
                      onClick={isDir ? () => setCurrentPath(f.path) : undefined}
                    >
                      <td class="px-4 py-2 font-mono text-gray-300">
                        {isDir ? (
                          <span class="text-amber-400">{name}/</span>
                        ) : (
                          name
                        )}
                      </td>
                      <td class="px-4 py-2 text-right text-gray-500 whitespace-nowrap">
                        {isDir ? "-" : formatBytes(f.size)}
                      </td>
                      <td class="px-4 py-2 text-right text-gray-600 whitespace-nowrap hidden sm:table-cell">
                        {f.mtime ? formatTimestamp(f.mtime) : "-"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
