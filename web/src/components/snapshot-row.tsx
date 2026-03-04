import { useState } from "preact/hooks";
import { formatTimestamp, type Snapshot, type RestoreProgress } from "../api";
import { PathPicker } from "./path-picker";

interface Props {
  snapshot: Snapshot;
  restoring: boolean;
  onRestore: (id: string, path: string) => void;
  onRestoreDb?: (id: string) => void;
  compact?: boolean;
  /** Pre-filled restore path (original data location) */
  defaultRestorePath?: string;
  /** Whether this snapshot is a database dump */
  isDbDump?: boolean;
  /** Active restore progress for this snapshot, if any */
  restoreProgress?: RestoreProgress | null;
}

export function SnapshotRow({
  snapshot,
  restoring,
  onRestore,
  onRestoreDb,
  compact = false,
  defaultRestorePath,
  isDbDump = false,
  restoreProgress,
}: Props) {
  const [showRestore, setShowRestore] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [confirmText, setConfirmText] = useState("");

  const bg = compact ? "bg-gray-800" : "bg-gray-900";
  const tagBg = compact ? "bg-gray-700" : "bg-gray-800";
  const btnSize = compact ? "text-xs px-2 py-1" : "text-sm px-3 py-1.5";
  const borderColor = compact ? "border-gray-700" : "border-gray-800";

  const initialPath = defaultRestorePath || "/";
  // Check if current selection matches the original data path
  const effectivePath = selectedPath ?? initialPath;
  const isOriginalPath = defaultRestorePath && (() => {
    const norm = (p: string) => p.replace(/\/+$/, "");
    const a = norm(effectivePath);
    const b = norm(defaultRestorePath);
    if (a === b) return true;
    // Handle ~ → /home/user expansion
    if (b.startsWith("~")) return a.endsWith(b.slice(1));
    return false;
  })();

  const handleRestore = () => {
    onRestore(snapshot.id, effectivePath);
    setShowRestore(false);
    setSelectedPath(null);
    setConfirmText("");
  };

  const handleDbRestore = () => {
    onRestoreDb?.(snapshot.id);
    setShowRestore(false);
    setConfirmText("");
  };

  return (
    <div class={`${bg} rounded-lg p-3`}>
      <div class="flex items-center justify-between">
        <div class="min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            {snapshot.source && (
              <span class={`text-xs px-1.5 py-0.5 rounded ${snapshot.source === "local" ? "bg-green-900/50 text-green-400" : "bg-blue-900/50 text-blue-400"}`}>
                {snapshot.source === "local" ? "Local" : "S3"}
              </span>
            )}
            {isDbDump && (
              <span class="text-xs px-1.5 py-0.5 rounded bg-purple-900/50 text-purple-400">
                DB
              </span>
            )}
            <a
              href={`/backups/snapshot/${snapshot.id}`}
              class="text-gray-300 font-mono text-sm hover:text-amber-400"
            >
              {snapshot.id.slice(0, 8)}
            </a>
            <span class="text-gray-500 text-sm">
              {formatTimestamp(snapshot.time)}
            </span>
          </div>
          {snapshot.tags.length > 0 && (
            <div class="flex gap-1.5 mt-1 flex-wrap">
              {snapshot.tags.map((tag) => (
                <span
                  key={tag}
                  class={`text-xs ${tagBg} text-gray-400 px-1.5 py-0.5 rounded`}
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>
        <button
          type="button"
          class={`${btnSize} bg-gray-700 hover:bg-gray-600 text-gray-300 rounded disabled:opacity-50 shrink-0`}
          disabled={restoring}
          onClick={() => {
            setShowRestore(!showRestore);
            setSelectedPath(null);
            setConfirmText("");
          }}
        >
          {restoring ? "Restoring..." : showRestore ? "Cancel" : isDbDump ? "Restore to DB" : "Restore"}
        </button>
      </div>
      {/* Restore progress indicator */}
      {restoreProgress && restoreProgress.status === "running" && (
        <div class={`mt-2 border-t ${borderColor} pt-2`}>
          <div class="flex items-center gap-2">
            <svg class="animate-spin h-3 w-3 text-blue-400 shrink-0" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
            <span class="text-xs text-blue-400 capitalize">{restoreProgress.phase}...</span>
          </div>
          {restoreProgress.log_lines.length > 0 && (
            <p class="text-xs text-gray-600 mt-1 truncate">{restoreProgress.log_lines[restoreProgress.log_lines.length - 1]}</p>
          )}
        </div>
      )}
      {restoreProgress && restoreProgress.status === "succeeded" && (
        <div class={`mt-2 border-t ${borderColor} pt-2`}>
          <span class="text-xs text-green-400">Restore completed successfully</span>
        </div>
      )}
      {restoreProgress && restoreProgress.status === "failed" && (
        <div class={`mt-2 border-t ${borderColor} pt-2`}>
          <span class="text-xs text-red-400">Restore failed: {restoreProgress.error || "Unknown error"}</span>
        </div>
      )}
      {showRestore && !restoring && isDbDump && (
        <div class={`mt-3 border-t ${borderColor} pt-3 space-y-3`}>
          <div class="bg-amber-900/20 border border-amber-500/30 rounded-lg p-3 space-y-2">
            <p class="text-sm text-amber-300">
              This will import the backup into the running database container.
            </p>
            <p class="text-xs text-gray-400">
              Type <span class="font-mono text-amber-400">restore</span> to confirm.
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
                class="px-3 py-1.5 bg-amber-600 hover:bg-amber-500 text-white text-sm rounded disabled:opacity-50"
                disabled={confirmText !== "restore"}
                onClick={handleDbRestore}
              >
                Restore to Database
              </button>
            </div>
          </div>
        </div>
      )}
      {showRestore && !restoring && !isDbDump && (
        <div class={`mt-3 border-t ${borderColor} pt-3 space-y-3`}>
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

          {/* Overwrite warning + confirmation when restoring to original path */}
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

          {/* Simple restore button for non-original paths */}
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
    </div>
  );
}
