import { useState, useEffect } from "preact/hooks";
import { route } from "preact-router";
import {
  api,
  formatTimestamp,
  formatBytes,
  type Snapshot,
  type SnapshotFile,
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

  const handleRestore = async (targetPath: string) => {
    if (!id) return;
    setRestoring(true);
    try {
      await api.backupRestore(id, targetPath);
      setShowRestore(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Restore failed");
    } finally {
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
            {snapshot.tags.length > 0 && (
              <div class="flex gap-1.5 flex-wrap">
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
          onClick={() => setShowRestore(!showRestore)}
        >
          {restoring ? "Restoring..." : showRestore ? "Cancel Restore" : "Restore"}
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

      {/* Restore path picker */}
      {showRestore && !restoring && (
        <div class="bg-gray-900 rounded-lg p-4">
          <p class="text-sm text-gray-400 mb-2">
            Select a directory to restore into:
          </p>
          <PathPicker
            onSelect={handleRestore}
            onCancel={() => setShowRestore(false)}
          />
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
