import { useState } from "preact/hooks";
import { api, formatBytes, type StorageVolumeStatus } from "../api";
import { PathPicker } from "./path-picker";

interface Props {
  vol: StorageVolumeStatus;
  appId: string;
  onUpdated: () => void;
}

export function StorageRow({ vol, appId, onUpdated }: Props) {
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);

  const handlePathSelect = async (path: string) => {
    setSaving(true);
    try {
      await api.updateStorage(appId, { [vol.name]: path });
      // Restart app so the new path takes effect
      api.deployApp(appId).catch(() => {});
      onUpdated();
      setEditing(false);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="bg-gray-900 rounded-lg px-4 py-3">
      <div class="flex items-center justify-between">
        <div class="min-w-0 mr-3">
          <span class="text-gray-200">{vol.description || vol.name}</span>
          <p class="text-xs text-gray-500 font-mono truncate">
            {vol.host_path || "Not configured"}
          </p>
        </div>
        <div class="flex items-center gap-3 shrink-0">
          {vol.disk_available_bytes != null && (
            <span class="text-sm text-gray-400">
              {formatBytes(vol.disk_available_bytes)} free
            </span>
          )}
          <button
            class="text-xs text-blue-400 hover:text-blue-300"
            onClick={() => setEditing(!editing)}
          >
            {editing ? "Cancel" : "Change"}
          </button>
        </div>
      </div>
      {editing && (
        <div class="mt-3 space-y-2">
          <p class="text-xs text-yellow-400">
            Changing storage won't move existing files. The app will restart.
          </p>
          {saving ? (
            <p class="text-gray-500 text-sm">Saving...</p>
          ) : (
            <PathPicker
              initialPath={vol.host_path || "/"}
              onSelect={handlePathSelect}
              onCancel={() => setEditing(false)}
            />
          )}
        </div>
      )}
    </div>
  );
}
