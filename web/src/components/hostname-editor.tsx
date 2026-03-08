import { useState, useEffect } from "preact/hooks";

interface Props {
  hostname: string;
  editing: boolean;
  saving: boolean;
  onSave(name: string): void;
  onStartEdit(): void;
  onCancel(): void;
}

export function HostnameEditor({
  hostname,
  editing,
  saving,
  onSave,
  onStartEdit,
  onCancel,
}: Props) {
  const [input, setInput] = useState(hostname);

  // Sync input when entering edit mode or hostname changes externally
  useEffect(() => {
    if (editing) setInput(hostname);
  }, [editing, hostname]);

  if (editing) {
    return (
      <>
        <input
          type="text"
          value={input}
          disabled={saving}
          onInput={(e) => setInput((e.target as HTMLInputElement).value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !saving) {
              onSave(input.trim());
            } else if (e.key === "Escape" && !saving) {
              onCancel();
            }
          }}
          class="text-xs text-gray-300 bg-gray-800 border border-gray-700 rounded px-2 py-1 focus:outline-none focus:border-gray-500 flex-1 disabled:opacity-50"
          autoFocus
        />
        <button
          class="px-3 py-1 text-xs rounded bg-gray-600 hover:bg-gray-500 text-gray-200 disabled:opacity-50"
          disabled={saving}
          onClick={() => onSave(input.trim())}
        >
          {saving ? "Saving..." : "Save"}
        </button>
        <button
          class="px-3 py-1 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-400 disabled:opacity-50"
          disabled={saving}
          onClick={onCancel}
        >
          Cancel
        </button>
      </>
    );
  }

  return (
    <>
      <span class="text-xs text-gray-300">{hostname}</span>
      <button
        class="text-xs text-gray-500 hover:text-gray-300"
        onClick={onStartEdit}
      >
        Rename
      </button>
    </>
  );
}
