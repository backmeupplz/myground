import { useState } from "preact/hooks";
import { formatTimestamp, type Snapshot } from "../api";
import { PathPicker } from "./path-picker";

interface Props {
  snapshot: Snapshot;
  restoring: boolean;
  onRestore: (id: string, path: string) => void;
  compact?: boolean;
}

export function SnapshotRow({
  snapshot,
  restoring,
  onRestore,
  compact = false,
}: Props) {
  const [showRestore, setShowRestore] = useState(false);

  const bg = compact ? "bg-gray-800" : "bg-gray-900";
  const tagBg = compact ? "bg-gray-700" : "bg-gray-800";
  const btnSize = compact ? "text-xs px-2 py-1" : "text-sm px-3 py-1.5";
  const borderColor = compact ? "border-gray-700" : "border-gray-800";

  return (
    <div class={`${bg} rounded-lg p-3`}>
      <div class="flex items-center justify-between">
        <div class="min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
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
          onClick={() => setShowRestore(!showRestore)}
        >
          {restoring ? "Restoring..." : showRestore ? "Cancel" : "Restore"}
        </button>
      </div>
      {showRestore && !restoring && (
        <div class={`mt-3 border-t ${borderColor} pt-3`}>
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
