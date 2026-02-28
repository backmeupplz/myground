import { useState, useEffect, useRef } from "preact/hooks";
import {
  api,
  type ServiceBackupConfig,
  type ContainerStatus,
  type InstallVariable,
} from "../api";
import { PathPicker } from "./path-picker";
import { LogViewer } from "./log-viewer";

type Step =
  | "pick-path"
  | "variables"
  | "backup"
  | "confirm"
  | "installing"
  | "deploying"
  | "starting";

interface Props {
  serviceId: string;
  serviceName: string;
  hasStorage: boolean;
  backupSupported: boolean;
  installVariables: InstallVariable[];
  onClose: () => void;
  onInstalled: () => void;
}

function containerColor(c: ContainerStatus): string {
  if (c.state === "running") return "text-green-400";
  if (c.state === "created") return "text-gray-400";
  return "text-red-400";
}

function containerIcon(c: ContainerStatus): string {
  if (c.state === "running") return "\u2713";
  return "\u25cb";
}

function isReady(containers: ContainerStatus[]): boolean {
  if (containers.length === 0) return false;
  return containers.every((c) => c.state === "running");
}

function isCrashLooping(containers: ContainerStatus[]): boolean {
  return containers.some(
    (c) =>
      c.status.includes("Restarting") ||
      c.state === "exited" ||
      c.state === "dead",
  );
}

function generatePassword(length: number): string {
  const chars =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.~";
  const arr = new Uint8Array(length);
  crypto.getRandomValues(arr);
  return Array.from(arr, (b) => chars[b % chars.length]).join("");
}

function computeInitialStep(
  hasStorage: boolean,
  installVariables: InstallVariable[],
  backupSupported: boolean,
): Step {
  if (hasStorage) return "pick-path";
  if (installVariables.length > 0) return "variables";
  if (backupSupported) return "backup";
  return "confirm";
}

function nextStepAfterPath(
  installVariables: InstallVariable[],
  backupSupported: boolean,
): Step {
  if (installVariables.length > 0) return "variables";
  if (backupSupported) return "backup";
  return "confirm";
}

function nextStepAfterVariables(backupSupported: boolean): Step {
  return backupSupported ? "backup" : "confirm";
}

export function InstallModal({
  serviceId,
  serviceName,
  hasStorage,
  backupSupported,
  installVariables,
  onClose,
  onInstalled,
}: Props) {
  const initialStep = computeInitialStep(
    hasStorage,
    installVariables,
    backupSupported,
  );
  const [step, setStep] = useState<Step>(initialStep);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [variables, setVariables] = useState<Record<string, string>>(() => {
    const init: Record<string, string> = {};
    for (const v of installVariables) {
      if (v.input_type === "password") {
        init[v.key] = generatePassword(25);
      } else {
        init[v.key] = v.default ?? "";
      }
    }
    return init;
  });
  const [backupConfig, setBackupConfig] = useState<ServiceBackupConfig>({
    enabled: false,
  });
  const [error, setError] = useState<string | null>(null);
  const [deployLines, setDeployLines] = useState<string[]>([]);
  const [containers, setContainers] = useState<ContainerStatus[]>([]);
  const [instanceId, setInstanceId] = useState<string | null>(null);
  const [editingPath, setEditingPath] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const logEndRef = useRef<HTMLDivElement>(null);

  const afterPath = nextStepAfterPath(installVariables, backupSupported);
  const afterVariables = nextStepAfterVariables(backupSupported);

  // Auto-scroll deploy log
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [deployLines]);

  // Poll container status while in "starting" step
  useEffect(() => {
    if (step !== "starting" || !instanceId) return;

    const poll = () => {
      api
        .services()
        .then((all) => {
          const svc = all.find((s) => s.id === instanceId);
          if (svc) setContainers(svc.containers);
        })
        .catch(() => {});
    };

    poll();
    pollRef.current = setInterval(poll, 1000);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [step, instanceId]);

  const handlePathSelected = (path: string) => {
    setSelectedPath(path);
    setStep(afterPath);
  };

  const variablesValid = installVariables.every(
    (v) => !v.required || (variables[v.key] ?? "").trim() !== "",
  );

  const handleInstall = async () => {
    setStep("installing");
    setError(null);
    try {
      const body: {
        storage_path?: string;
        variables?: Record<string, string>;
      } = {};
      if (selectedPath) body.storage_path = selectedPath;
      if (installVariables.length > 0) body.variables = variables;

      const result = await api.installService(serviceId, body);
      const id = result.message
        .replace("Service ", "")
        .replace(" installed", "");
      setInstanceId(id);

      if (backupConfig.enabled || backupConfig.remote) {
        try {
          await api.updateServiceBackup(id, backupConfig);
        } catch {
          // Non-fatal
        }
      }

      onInstalled();

      // Connect to deploy WebSocket
      setStep("deploying");
      setDeployLines([]);
      const wsProtocol =
        window.location.protocol === "https:" ? "wss:" : "ws:";
      const ws = new WebSocket(
        `${wsProtocol}//${window.location.host}/api/services/${id}/deploy`,
      );

      ws.onmessage = (event) => {
        const line = event.data;
        if (line === "__DONE__") {
          ws.close();
          setStep("starting");
          return;
        }
        setDeployLines((prev) => [...prev, line]);
      };

      ws.onerror = () => {
        setDeployLines((prev) => [...prev, "Connection error"]);
      };

      ws.onclose = () => {
        setStep((s) => (s === "deploying" ? "starting" : s));
      };
    } catch (e) {
      setError(e instanceof Error ? e.message : "Install failed");
      setStep("confirm");
    }
  };

  const ready = isReady(containers);
  const crashing = isCrashLooping(containers);

  const stepTitles: Record<Step, string> = {
    "pick-path": "Choose storage location",
    variables: "Configure service",
    backup: "Set up backups?",
    confirm: "Confirm Install",
    installing: "Configuring...",
    deploying: "Deploying...",
    starting: ready ? "Ready!" : crashing ? "Failed to start" : "Starting...",
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
          <h2 class="text-lg font-bold text-gray-100">{stepTitles[step]}</h2>
          {step !== "installing" && (
            <button
              class="text-gray-500 hover:text-gray-300 text-xl"
              onClick={onClose}
            >
              &times;
            </button>
          )}
        </div>

        {/* Step: Path picker */}
        {step === "pick-path" && (
          <div class="space-y-3">
            <p class="text-sm text-gray-400">
              Browse to a folder or type a path. Data will be stored under this
              location.
            </p>
            <PathPicker
              onSelect={handlePathSelected}
              onCancel={() => {
                setSelectedPath(null);
                setStep(afterPath);
              }}
            />
          </div>
        )}

        {/* Step: Install variables */}
        {step === "variables" && (
          <div class="space-y-4">
            {installVariables.map((v) => (
              <div key={v.key}>
                <label class="text-xs text-gray-500 block mb-1">
                  {v.label}
                  {v.required && <span class="text-red-400 ml-1">*</span>}
                </label>
                {v.input_type === "path" ? (
                  editingPath === v.key ? (
                    <PathPicker
                      initialPath={variables[v.key] || "/"}
                      onSelect={(path) => {
                        setVariables((prev) => ({ ...prev, [v.key]: path }));
                        setEditingPath(null);
                      }}
                      onCancel={() => setEditingPath(null)}
                    />
                  ) : (
                    <div class="flex items-center gap-2">
                      <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate">
                        {variables[v.key] || v.default || "/"}
                      </span>
                      <button
                        class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                        onClick={() => setEditingPath(v.key)}
                      >
                        Browse
                      </button>
                    </div>
                  )
                ) : (
                  <input
                    type={v.input_type === "password" ? "password" : "text"}
                    value={variables[v.key] ?? ""}
                    onInput={(e) =>
                      setVariables((prev) => ({
                        ...prev,
                        [v.key]: (e.target as HTMLInputElement).value,
                      }))
                    }
                    class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono"
                    placeholder={v.default ?? ""}
                  />
                )}
              </div>
            ))}
            <div class="flex gap-3 pt-2">
              <button
                class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
                onClick={() => setStep(afterVariables)}
                disabled={!variablesValid}
              >
                Next
              </button>
            </div>
          </div>
        )}

        {/* Step: Backup config */}
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

        {/* Step: Confirm */}
        {step === "confirm" && (
          <div>
            <div class="space-y-2 text-sm text-gray-300 mb-4">
              <p>
                Service:{" "}
                <span class="font-semibold text-gray-100">{serviceName}</span>
              </p>
              {hasStorage && (
                <p>
                  Storage:{" "}
                  <span class="font-mono text-gray-100 break-all">
                    {selectedPath ?? "Default"}
                  </span>
                </p>
              )}
              {installVariables.length > 0 && (
                <div>
                  {installVariables.map((v) => (
                    <p key={v.key}>
                      {v.label}:{" "}
                      <span class="font-mono text-gray-100 break-all">
                        {v.input_type === "password"
                          ? "\u2022".repeat(8)
                          : variables[v.key] || v.default || ""}
                      </span>
                    </p>
                  ))}
                </div>
              )}
              {backupSupported && (
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
              )}
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

        {/* Configuring: setup API call in flight */}
        {step === "installing" && (
          <div class="text-center py-4">
            <p class="text-gray-300">Configuring {serviceName}...</p>
          </div>
        )}

        {/* Deploying: streaming docker compose output */}
        {step === "deploying" && (
          <div class="space-y-3">
            <div class="bg-gray-950 rounded-lg p-3 max-h-60 overflow-y-auto font-mono text-xs text-gray-400">
              {deployLines.map((line, i) => (
                <div
                  key={i}
                  class={
                    line.startsWith("Pulling") || line.startsWith("Starting")
                      ? "text-blue-400 font-semibold mt-1"
                      : line.startsWith("Error")
                        ? "text-red-400"
                        : ""
                  }
                >
                  {line}
                </div>
              ))}
              <div ref={logEndRef} />
            </div>
            <button
              class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded"
              onClick={onClose}
            >
              Close
            </button>
          </div>
        )}

        {/* Starting: live container status + logs if crashing */}
        {step === "starting" && (
          <div class="space-y-4">
            <div class="space-y-2">
              {containers.length === 0 && (
                <p class="text-gray-500 text-sm">
                  Waiting for containers...
                </p>
              )}
              {containers.map((c) => (
                <div
                  key={c.name}
                  class="flex items-center gap-3 bg-gray-800 rounded-lg px-4 py-3"
                >
                  <span class={`text-lg ${containerColor(c)}`}>
                    {containerIcon(c)}
                  </span>
                  <div class="min-w-0">
                    <p class="text-sm text-gray-200 truncate">{c.name}</p>
                    <p class={`text-xs ${containerColor(c)}`}>{c.status}</p>
                  </div>
                </div>
              ))}
            </div>

            {crashing && (
              <p class="text-sm text-red-400">
                Service failed to start. Check logs below for details.
              </p>
            )}

            {instanceId && containers.length > 0 && (
              <LogViewer serviceId={instanceId} />
            )}

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
