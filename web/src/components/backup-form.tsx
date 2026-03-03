import { useState, useEffect } from "preact/hooks";
import { api, type AppBackupConfig } from "../api";
import { BackupConfigFields } from "./backup-config-fields";

interface Props {
  appId: string;
  hasBackupPassword: boolean;
}

export function BackupForm({ appId, hasBackupPassword }: Props) {
  const [config, setConfig] = useState<AppBackupConfig>({
    enabled: false,
    local: [],
    remote: [],
  });
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [password, setPassword] = useState<string | null>(null);
  const [loadingPw, setLoadingPw] = useState(false);

  useEffect(() => {
    api
      .getAppBackup(appId)
      .then(setConfig)
      .catch(() => {});
  }, [appId]);

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    try {
      await api.updateAppBackup(appId, config);
      setMessage("Saved");
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const handleReveal = async () => {
    setLoadingPw(true);
    try {
      const res = await api.getBackupPassword(appId);
      setPassword(res.password);
    } catch {
      setPassword(null);
    } finally {
      setLoadingPw(false);
    }
  };

  const hasRepo =
    config.local.some((c) => c.repository) ||
    config.remote.some((c) => c.repository);
  const hasConfigured = (config.local.length > 0 || config.remote.length > 0) && hasRepo;

  return (
    <div class="space-y-4">
      <BackupConfigFields config={config} onChange={setConfig} />
      {hasBackupPassword && hasConfigured && (
        <div class="bg-gray-800 rounded-lg px-4 py-3 flex items-center justify-between">
          <div class="min-w-0 mr-3">
            <span class="text-gray-200 text-sm">Encryption Password</span>
            <p class="text-xs text-gray-500 font-mono truncate">
              {"\u2022".repeat(8)}
            </p>
          </div>
          {password ? (
            <CopyButton value={password} />
          ) : (
            <button
              type="button"
              class="text-xs text-blue-400 hover:text-blue-300 shrink-0"
              disabled={loadingPw}
              onClick={handleReveal}
            >
              {loadingPw ? "..." : "Reveal"}
            </button>
          )}
        </div>
      )}
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

function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      class="text-xs text-blue-400 hover:text-blue-300 shrink-0"
      onClick={() => {
        navigator.clipboard.writeText(value).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        });
      }}
    >
      {copied ? "Copied!" : "Copy"}
    </button>
  );
}
