import { useState, useEffect } from "preact/hooks";
import { api, type DirEntry } from "../api";

interface Props {
  initialPath?: string;
  onSelect: (path: string) => void;
  onCancel?: () => void;
}

export function PathPicker({ initialPath = "/", onSelect, onCancel }: Props) {
  const [currentPath, setCurrentPath] = useState(initialPath);
  const [inputValue, setInputValue] = useState(initialPath);
  const [entries, setEntries] = useState<DirEntry[]>([]);
  const [loading, setLoading] = useState(true);

  const browse = (path: string) => {
    setLoading(true);
    api
      .browse(path)
      .then((result) => {
        setCurrentPath(result.path);
        setInputValue(result.path);
        setEntries(result.entries);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  };

  useEffect(() => {
    browse(initialPath);
  }, []);

  const handleInputKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter") {
      browse(inputValue);
    }
  };

  const parentPath = currentPath === "/" ? null : currentPath.replace(/\/[^/]+\/?$/, "") || "/";

  return (
    <div class="space-y-3">
      {/* Path input */}
      <div class="flex gap-2">
        <input
          type="text"
          value={inputValue}
          onInput={(e) => setInputValue((e.target as HTMLInputElement).value)}
          onKeyDown={handleInputKeyDown}
          class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-2 text-sm text-gray-200 font-mono min-w-0"
          placeholder="/path/to/directory"
        />
        <button
          class="px-3 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
          onClick={() => browse(inputValue)}
        >
          Go
        </button>
      </div>

      {/* Directory listing */}
      <div class="bg-gray-800 rounded-lg overflow-hidden max-h-60 overflow-y-auto">
        {loading ? (
          <p class="text-gray-500 text-sm p-3">Loading...</p>
        ) : (
          <div>
            {parentPath !== null && (
              <button
                class="w-full px-3 py-2 text-left text-sm hover:bg-gray-700 flex items-center gap-2 text-gray-400 border-b border-gray-700/50"
                onClick={() => browse(parentPath)}
              >
                <span>&#8592;</span>
                <span>..</span>
              </button>
            )}
            {entries.length === 0 && (
              <p class="text-gray-500 text-xs p-3">No subdirectories</p>
            )}
            {entries.map((entry) => (
              <button
                key={entry.path}
                class="w-full px-3 py-2 text-left text-sm hover:bg-gray-700 flex items-center gap-2 text-gray-200 border-b border-gray-700/50 last:border-0"
                onClick={() => browse(entry.path)}
              >
                <span class="text-gray-500 shrink-0">&#128193;</span>
                <span class="truncate">{entry.name}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Actions */}
      <div class="flex gap-3">
        <button
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
          onClick={() => onSelect(currentPath)}
        >
          Select this folder
        </button>
        {onCancel && (
          <button
            class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
            onClick={onCancel}
          >
            Cancel
          </button>
        )}
      </div>
    </div>
  );
}
