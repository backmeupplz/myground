import { useState, useEffect, useRef } from "preact/hooks";
import {
  api,
  generatePassword,
  containerColor,
  containerIcon,
  isReady,
  isCrashLooping,
  type ServiceBackupConfig,
  type ContainerStatus,
  type InstallVariable,
} from "../api";
import { PathPicker } from "./path-picker";
import { LogViewer } from "./log-viewer";
import { BackupConfigFields } from "./backup-config-fields";

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

function computeFirstStep(
  hasStorage: boolean,
  hasVariables: boolean,
  backupSupported: boolean,
): Step {
  if (hasStorage) return "pick-path";
  if (hasVariables) return "variables";
  if (backupSupported) return "backup";
  return "confirm";
}

function nextStep(hasVariables: boolean, backupSupported: boolean, after: "path" | "variables"): Step {
  if (after === "path" && hasVariables) return "variables";
  if (backupSupported) return "backup";
  return "confirm";
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
  const hasVars = installVariables.length > 0;
  const [step, setStep] = useState<Step>(
    computeFirstStep(hasStorage, hasVars, backupSupported),
  );
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [variables, setVariables] = useState<Record<string, string>>(() => {
    const init: Record<string, string> = {};
    for (const v of installVariables) {
      init[v.key] = v.input_type === "password" ? generatePassword(25) : (v.default ?? "");
    }
    return init;
  });
  const [displayName, setDisplayName] = useState(serviceName);
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

  const afterPath = nextStep(hasVars, backupSupported, "path");
  const afterVariables = nextStep(hasVars, backupSupported, "variables");

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
        display_name?: string;
      } = {};
      if (selectedPath) body.storage_path = selectedPath;
      if (hasVars) body.variables = variables;
      if (displayName.trim() && displayName.trim() !== serviceName) {
        body.display_name = displayName.trim();
      }

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
              onSelect={(path) => {
                setSelectedPath(path);
                setStep(afterPath);
              }}
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
            <BackupConfigFields
              config={backupConfig}
              onChange={setBackupConfig}
            />
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
            <div class="space-y-3 text-sm text-gray-300 mb-4">
              <div>
                <label class="text-xs text-gray-500 block mb-1">Name</label>
                <input
                  type="text"
                  value={displayName}
                  onInput={(e) =>
                    setDisplayName((e.target as HTMLInputElement).value)
                  }
                  class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
                  placeholder={serviceName}
                />
              </div>
              {hasStorage && (
                <p>
                  Storage:{" "}
                  <span class="font-mono text-gray-100 break-all">
                    {selectedPath ?? "Default"}
                  </span>
                </p>
              )}
              {hasVars && (
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
