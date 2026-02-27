import { useState, useEffect } from "preact/hooks";
import {
  api,
  formatBytes,
  type DiskInfo,
  type ServiceBackupConfig,
} from "../api";

type Step = "pick-disk" | "backup" | "confirm" | "installing" | "done";

interface Props {
  serviceId: string;
  serviceName: string;
  hasStorage: boolean;
  onClose: () => void;
  onInstalled: () => void;
}

export function InstallModal({
  serviceId,
  serviceName,
  hasStorage,
  onClose,
  onInstalled,
}: Props) {
  const initialStep: Step = hasStorage ? "pick-disk" : "backup";
  const [step, setStep] = useState<Step>(initialStep);
  const [disks, setDisks] = useState<DiskInfo[]>([]);
  const [selectedDisk, setSelectedDisk] = useState<string | null>(null);
  const [backupConfig, setBackupConfig] = useState<ServiceBackupConfig>({
    enabled: false,
  });
  const [error, setError] = useState<string | null>(null);
  const [resultMessage, setResultMessage] = useState("");

  useEffect(() => {
    if (hasStorage) {
      api.disks().then(setDisks).catch(() => setDisks([]));
    }
  }, [hasStorage]);

  const handlePickDisk = (mountPoint: string) => {
    setSelectedDisk(mountPoint);
    setStep("backup");
  };

  const handleInstall = async () => {
    setStep("installing");
    setError(null);
    try {
      const body = selectedDisk ? { storage_path: selectedDisk } : undefined;
      const result = await api.installService(serviceId, body);

      // Save backup config if enabled
      if (backupConfig.enabled || backupConfig.remote) {
        try {
          await api.updateServiceBackup(serviceId, backupConfig);
        } catch {
          // Non-fatal: service is installed, backup config just didn't save
        }
      }

      setResultMessage(result.message);
      setStep("done");
      onInstalled();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Install failed");
      setStep("confirm");
    }
  };

  const stepTitles: Record<Step, string> = {
    "pick-disk": "Where to store data?",
    backup: "Set up backups?",
    confirm: "Confirm Install",
    installing: "Installing...",
    done: "Installed!",
  };

  return (
    <div
      class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        class="bg-gray-900 rounded-xl max-w-lg w-full p-6 max-h-[80vh] overflow-y-auto"
        onClick={(e: Event) => e.stopPropagation()}
      >
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-lg font-bold text-gray-100">
            {stepTitles[step]}
          </h2>
          {step !== "installing" && (
            <button
              class="text-gray-500 hover:text-gray-300 text-xl"
              onClick={onClose}
            >
              &times;
            </button>
          )}
        </div>

        {/* Step 1: Disk picker */}
        {step === "pick-disk" && (
          <div class="space-y-3">
            {disks.map((disk) => (
              <button
                key={disk.mount_point}
                class="w-full bg-gray-800 hover:bg-gray-700 rounded-lg p-4 text-left transition-colors flex items-center justify-between"
                onClick={() => handlePickDisk(disk.mount_point)}
              >
                <div>
                  <h3 class="font-semibold text-gray-100">
                    {disk.mount_point}
                  </h3>
                  <p class="text-xs text-gray-500">{disk.fs_type}</p>
                </div>
                <span class="text-sm text-gray-400">
                  {formatBytes(disk.available_bytes)} free
                </span>
              </button>
            ))}
            <button
              class="w-full bg-gray-800/50 hover:bg-gray-700 rounded-lg p-4 text-left text-gray-400 transition-colors"
              onClick={() => {
                setSelectedDisk(null);
                setStep("backup");
              }}
            >
              Use default location
            </button>
          </div>
        )}

        {/* Step 2: Backup config */}
        {step === "backup" && (
          <div class="space-y-4">
            <p class="text-sm text-gray-400">
              You can configure backups now or later from the service detail
              page.
            </p>

            <label class="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={backupConfig.enabled}
                onChange={(e) =>
                  setBackupConfig({
                    ...backupConfig,
                    enabled: (e.target as HTMLInputElement).checked,
                  })
                }
                class="rounded bg-gray-700 border-gray-600"
              />
              <span class="text-gray-300">Enable local backups</span>
            </label>

            {backupConfig.enabled && (
              <div class="pl-6 space-y-3">
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Repository path
                  </label>
                  <input
                    type="text"
                    value={backupConfig.local?.repository ?? ""}
                    onInput={(e) =>
                      setBackupConfig({
                        ...backupConfig,
                        local: {
                          ...backupConfig.local,
                          repository: (e.target as HTMLInputElement).value,
                        },
                      })
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                    placeholder="/mnt/backups"
                  />
                </div>
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Password
                  </label>
                  <input
                    type="password"
                    value={backupConfig.local?.password ?? ""}
                    onInput={(e) =>
                      setBackupConfig({
                        ...backupConfig,
                        local: {
                          ...backupConfig.local,
                          password: (e.target as HTMLInputElement).value,
                        },
                      })
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                  />
                </div>
              </div>
            )}

            <label class="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={!!backupConfig.remote}
                onChange={(e) => {
                  const checked = (e.target as HTMLInputElement).checked;
                  setBackupConfig({
                    ...backupConfig,
                    remote: checked ? {} : undefined,
                  });
                }}
                class="rounded bg-gray-700 border-gray-600"
              />
              <span class="text-gray-300">Enable cloud backups (S3)</span>
            </label>

            {backupConfig.remote && (
              <div class="pl-6 space-y-3">
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Bucket URL
                  </label>
                  <input
                    type="text"
                    value={backupConfig.remote.repository ?? ""}
                    onInput={(e) =>
                      setBackupConfig({
                        ...backupConfig,
                        remote: {
                          ...backupConfig.remote,
                          repository: (e.target as HTMLInputElement).value,
                        },
                      })
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                    placeholder="s3:https://s3.amazonaws.com/mybucket"
                  />
                </div>
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Access Key
                  </label>
                  <input
                    type="text"
                    value={backupConfig.remote.s3_access_key ?? ""}
                    onInput={(e) =>
                      setBackupConfig({
                        ...backupConfig,
                        remote: {
                          ...backupConfig.remote,
                          s3_access_key: (e.target as HTMLInputElement).value,
                        },
                      })
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                  />
                </div>
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Secret Key
                  </label>
                  <input
                    type="password"
                    value={backupConfig.remote.s3_secret_key ?? ""}
                    onInput={(e) =>
                      setBackupConfig({
                        ...backupConfig,
                        remote: {
                          ...backupConfig.remote,
                          s3_secret_key: (e.target as HTMLInputElement).value,
                        },
                      })
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                  />
                </div>
                <div>
                  <label class="text-xs text-gray-500 block mb-1">
                    Password
                  </label>
                  <input
                    type="password"
                    value={backupConfig.remote.password ?? ""}
                    onInput={(e) =>
                      setBackupConfig({
                        ...backupConfig,
                        remote: {
                          ...backupConfig.remote,
                          password: (e.target as HTMLInputElement).value,
                        },
                      })
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                  />
                </div>
              </div>
            )}

            <div class="flex gap-3 pt-2">
              <button
                class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded"
                onClick={() => setStep("confirm")}
              >
                Next
              </button>
              <button
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
                onClick={() => {
                  setBackupConfig({ enabled: false });
                  setStep("confirm");
                }}
              >
                Skip
              </button>
            </div>
          </div>
        )}

        {/* Step 3: Confirm */}
        {step === "confirm" && (
          <div>
            <div class="space-y-2 text-sm text-gray-300 mb-4">
              <p>
                Service:{" "}
                <span class="font-semibold text-gray-100">{serviceName}</span>
              </p>
              {selectedDisk && (
                <p>
                  Storage:{" "}
                  <span class="font-mono text-gray-100">{selectedDisk}</span>
                </p>
              )}
              <p>
                Backups:{" "}
                <span class="text-gray-100">
                  {backupConfig.enabled || backupConfig.remote
                    ? [
                        backupConfig.enabled && "Local",
                        backupConfig.remote && "S3",
                      ]
                        .filter(Boolean)
                        .join(" + ")
                    : "None"}
                </span>
              </p>
            </div>
            {error && <p class="text-red-400 text-sm mb-3">{error}</p>}
            <div class="flex gap-3">
              <button
                class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded"
                onClick={handleInstall}
              >
                Install
              </button>
              <button
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={onClose}
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Installing state */}
        {step === "installing" && (
          <div class="text-center py-4">
            <p class="text-gray-300">Installing and starting {serviceName}...</p>
            <p class="text-sm text-gray-500 mt-2">
              This may take a few minutes while images are downloaded.
            </p>
          </div>
        )}

        {/* Done state */}
        {step === "done" && (
          <div>
            <p class="text-green-400 mb-4">{resultMessage}</p>
            <p class="text-sm text-gray-400 mb-4">
              The service is starting up. It may take a moment before it's
              fully ready.
            </p>
            <button
              class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
              onClick={onClose}
            >
              Close
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
