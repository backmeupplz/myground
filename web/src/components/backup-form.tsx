import { useState, useEffect } from "preact/hooks";
import { api, type ServiceBackupConfig } from "../api";
import { BackupConfigFields } from "./backup-config-fields";

interface Props {
  serviceId: string;
}

export function BackupForm({ serviceId }: Props) {
  const [config, setConfig] = useState<ServiceBackupConfig>({
    enabled: false,
  });
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    api
      .getServiceBackup(serviceId)
      .then(setConfig)
      .catch(() => {});
  }, [serviceId]);

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    try {
      await api.updateServiceBackup(serviceId, config);
      setMessage("Saved");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="space-y-4">
      <BackupConfigFields config={config} onChange={setConfig} />
      <div class="flex items-center gap-3 pt-2">
        <button
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
          disabled={saving}
          onClick={handleSave}
        >
          {saving ? "Saving..." : "Save"}
        </button>
        {message && (
          <span
            class={`text-sm ${message === "Saved" ? "text-green-400" : "text-red-400"}`}
          >
            {message}
          </span>
        )}
      </div>
    </div>
  );
}
