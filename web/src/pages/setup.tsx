import { useState } from "preact/hooks";
import {
  api,
  generatePassword,
  type AvailableApp,
  type GlobalConfig,
  type VpnConfig,
  type AwsSetupResult,
} from "../api";
import { PathPicker } from "../components/path-picker";
import { AppIcon } from "../components/app-icon";
import { TailscaleGuide } from "../components/tailscale-guide";
import { VariableField } from "../components/variable-field";
import { Field } from "../components/field";
import { AwsSetupForm } from "../components/aws-setup-form";

interface Props {
  onComplete: () => void;
}

type Step = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9;

const STEP_LABELS = [
  "Welcome",
  "Account",
  "Storage",
  "Tailscale",
  "VPN",
  "Cloudflare",
  "Backups",
  "Apps",
  "Done",
];

export function Setup({ onComplete }: Props) {
  const [step, setStep] = useState<Step>(1);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  // Step 2: Account
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");

  // Step 3: Storage
  const [storagePath, setStoragePath] = useState("");
  const [browsing, setBrowsing] = useState(false);

  // Step 4: Tailscale
  const [tailscaleKey, setTailscaleKey] = useState("");

  // Step 5: VPN
  const [vpnProvider, setVpnProvider] = useState("protonvpn");
  const [vpnType, setVpnType] = useState("openvpn");
  const [vpnCountry, setVpnCountry] = useState("");
  const [vpnPortForward, setVpnPortForward] = useState(true);
  const [vpnEnvVars, setVpnEnvVars] = useState<Record<string, string>>({});
  const [vpnTesting, setVpnTesting] = useState(false);
  const [vpnTestResult, setVpnTestResult] = useState<boolean | null>(null);
  const [vpnTestLogs, setVpnTestLogs] = useState<string[]>([]);

  // Step 6: Cloudflare
  const [cloudflareToken, setCloudflareToken] = useState("");
  const [cloudflareProgress, setCloudflareProgress] = useState("");

  // Step 7: Backups
  const [backupLocalEnabled, setBackupLocalEnabled] = useState(false);
  const [backupLocalPath, setBackupLocalPath] = useState("");
  const [backupRemoteEnabled, setBackupRemoteEnabled] = useState(false);
  const [backupRemoteRepo, setBackupRemoteRepo] = useState("");
  const [backupS3Key, setBackupS3Key] = useState("");
  const [backupS3Secret, setBackupS3Secret] = useState("");
  const [browsingBackupPath, setBrowsingBackupPath] = useState(false);

  // Step 8: Apps
  const [availableApps, setAvailableApps] = useState<
    AvailableApp[]
  >([]);
  const [selectedApps, setSelectedApps] = useState<Set<string>>(
    new Set(),
  );
  const [configPhase, setConfigPhase] = useState<"select" | "configure">(
    "select",
  );
  const [configIndex, setConfigIndex] = useState(0);
  const [allVariables, setAllVariables] = useState<
    Record<string, Record<string, string>>
  >({});
  const [allStoragePaths, setAllStoragePaths] = useState<
    Record<string, Record<string, string>>
  >({});
  const [browsingVolume, setBrowsingVolume] = useState<{
    appId: string;
    volName: string;
  } | null>(null);
  const [allMainPaths, setAllMainPaths] = useState<Record<string, string>>({});
  const [showAdvancedStorage, setShowAdvancedStorage] = useState<Record<string, boolean>>({});
  const [browsingMainPath, setBrowsingMainPath] = useState<string | null>(null);

  // Step 9: Summary
  const [configuredStorage, setConfiguredStorage] = useState<string | null>(
    null,
  );
  const [configuredTailscale, setConfiguredTailscale] = useState(false);
  const [configuredVpn, setConfiguredVpn] = useState(false);
  const [configuredCloudflare, setConfiguredCloudflare] = useState(false);
  const [configuredBackup, setConfiguredBackup] = useState(false);
  const [installedApps, setInstalledApps] = useState<string[]>([]);
  const [deploying, setDeploying] = useState(false);
  const [deployQueue, setDeployQueue] = useState<string[]>([]);
  const [deployDone, setDeployDone] = useState<string[]>([]);
  const [deployActive, setDeployActive] = useState<string[]>([]);

  const goTo = (s: Step) => {
    setError("");
    setStep(s);
  };

  // ── Step 2: Create account ──────────────────────────────────────────────

  const handleAccountSubmit = async (e: Event) => {
    e.preventDefault();
    setError("");

    if (!username.trim() || !password) {
      setError("Username and password are required.");
      return;
    }
    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }
    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    setLoading(true);
    try {
      await api.setup({ username: username.trim(), password });
      // Fetch global config to get the default storage path
      const config = await api.globalConfig();
      setStoragePath(config.default_storage_path ?? "");
      // Pre-fetch available apps for step 5
      const allApps = await api.availableApps();
      setAvailableApps(allApps);
      goTo(3);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Setup failed");
    } finally {
      setLoading(false);
    }
  };

  // ── Step 3: Storage path ────────────────────────────────────────────────

  const handleStorageSave = async (path: string) => {
    setLoading(true);
    setError("");
    try {
      const config = await api.globalConfig();
      const updated: GlobalConfig = { ...config, default_storage_path: path };
      await api.saveGlobalConfig(updated);
      setConfiguredStorage(path);
      goTo(4);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to save storage");
    } finally {
      setLoading(false);
    }
  };

  const handleStorageSkip = () => {
    setConfiguredStorage(storagePath || null);
    goTo(4);
  };

  // ── Step 4: Tailscale ───────────────────────────────────────────────────

  const handleTailscaleEnable = async () => {
    const key = tailscaleKey.trim();
    if (!key) {
      setError("Please enter a Tailscale auth key.");
      return;
    }
    if (!key.startsWith("tskey-auth-")) {
      setError("That doesn't look like a Tailscale auth key. It should start with \"tskey-auth-\".");
      return;
    }
    setLoading(true);
    setError("");
    try {
      await api.saveTailscaleConfig({ enabled: true, auth_key: key });
      setConfiguredTailscale(true);
      goTo(5 as Step);
    } catch (err: unknown) {
      setError(
        err instanceof Error ? err.message : "Failed to enable Tailscale",
      );
    } finally {
      setLoading(false);
    }
  };

  // ── Step 5: VPN ─────────────────────────────────────────────────────────

  const handleVpnEnable = async () => {
    if (vpnType === "openvpn") {
      if (!vpnEnvVars["OPENVPN_USER"]?.trim()) {
        setError("Please enter your OpenVPN username.");
        return;
      }
      if (!vpnEnvVars["OPENVPN_PASSWORD"]?.trim()) {
        setError("Please enter your OpenVPN password.");
        return;
      }
    } else if (vpnType === "wireguard") {
      if (!vpnEnvVars["WIREGUARD_PRIVATE_KEY"]?.trim()) {
        setError("Please enter your WireGuard private key.");
        return;
      }
    }
    setLoading(true);
    setError("");
    try {
      const config: VpnConfig = {
        enabled: true,
        provider: vpnProvider,
        vpn_type: vpnType,
        server_countries: vpnCountry || undefined,
        port_forwarding: vpnPortForward,
        env_vars: vpnEnvVars,
      };
      await api.saveVpnConfig(config);
      setConfiguredVpn(true);
      goTo(6 as Step);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to save VPN config");
    } finally {
      setLoading(false);
    }
  };

  // ── Step 6: Cloudflare ─────────────────────────────────────────────────

  const handleCloudflareEnable = async () => {
    const token = cloudflareToken.trim();
    if (!token) {
      setError("Please enter a Cloudflare API token.");
      return;
    }
    setLoading(true);
    setError("");
    setCloudflareProgress("Starting...");
    try {
      // Poll for real backend progress while the config call runs
      let done = false;
      const pollProgress = async () => {
        while (!done) {
          try {
            const st = await api.cloudflareStatus();
            if (st.setup_progress) setCloudflareProgress(st.setup_progress);
          } catch { /* ignore */ }
          await new Promise((r) => setTimeout(r, 1000));
        }
      };
      const pollPromise = pollProgress();

      await api.saveCloudflareConfig({ enabled: true, api_token: token });
      done = true;
      await pollPromise;

      setCloudflareProgress("Verifying tunnel is running...");
      // Poll until tunnel is running or timeout after 30s
      const deadline = Date.now() + 30_000;
      while (Date.now() < deadline) {
        try {
          const st = await api.cloudflareStatus();
          if (st.tunnel_running) break;
        } catch { /* keep polling */ }
        await new Promise((r) => setTimeout(r, 2000));
      }
      setCloudflareProgress("");
      setConfiguredCloudflare(true);
      goTo(7 as Step);
    } catch (err: unknown) {
      setCloudflareProgress("");
      setError(
        err instanceof Error ? err.message : "Failed to enable Cloudflare",
      );
    } finally {
      setLoading(false);
    }
  };

  // ── Step 7: Backups ────────────────────────────────────────────────────

  const handleBackupSave = async () => {
    setLoading(true);
    setError("");
    try {
      const config = await api.globalConfig();
      let default_local_destination = config.default_local_destination;
      let default_remote_destination = config.default_remote_destination;
      if (backupLocalEnabled && backupLocalPath) {
        default_local_destination = { ...default_local_destination, repository: backupLocalPath };
      }
      if (backupRemoteEnabled && backupRemoteRepo) {
        default_remote_destination = {
          ...default_remote_destination,
          repository: backupRemoteRepo,
          ...(backupS3Key ? { s3_access_key: backupS3Key } : {}),
          ...(backupS3Secret ? { s3_secret_key: backupS3Secret } : {}),
        };
      }
      await api.saveGlobalConfig({ ...config, default_local_destination, default_remote_destination });
      setConfiguredBackup(true);
      goTo(8 as Step);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to save backup config");
    } finally {
      setLoading(false);
    }
  };

  // ── Step 8: Install apps ────────────────────────────────────────────────

  const toggleApp = (id: string) => {
    setSelectedApps((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const resolveDefaultPath = (appId: string, volName: string, volCount: number): string => {
    const base = storagePath || "~/.myground/apps";
    if (volCount <= 1) {
      return `${base}/${appId}/`;
    }
    return `${base}/${appId}/${volName}/`;
  };

  const handleNextFromSelection = () => {
    const ids = Array.from(selectedApps);
    if (ids.length === 0) {
      goTo(9 as Step);
      return;
    }

    // Pre-populate allVariables with defaults for every selected app
    const vars: Record<string, Record<string, string>> = {};
    const paths: Record<string, Record<string, string>> = {};
    const mainPaths: Record<string, string> = {};
    const base = storagePath || "~/.myground/apps";
    for (const id of ids) {
      const svc = availableApps.find((s) => s.id === id);
      const variables: Record<string, string> = {};
      mainPaths[id] = `${base}/${id}`;
      if (svc) {
        for (const v of svc.install_variables) {
          if (v.input_type === "password") {
            variables[v.key] = generatePassword(25);
          } else if (v.default) {
            variables[v.key] = v.default;
          }
        }
        // Pre-populate storage paths with resolved defaults
        if (svc.storage_volumes.length > 0) {
          const volPaths: Record<string, string> = {};
          for (const vol of svc.storage_volumes) {
            volPaths[vol.name] = resolveDefaultPath(id, vol.name, svc.storage_volumes.length);
          }
          paths[id] = volPaths;
        }
      }
      vars[id] = variables;
    }
    setAllVariables(vars);
    setAllStoragePaths(paths);
    setAllMainPaths(mainPaths);

    // Check if any selected app has variables or storage volumes to configure
    const appsWithConfig = ids.filter((id) => {
      const svc = availableApps.find((s) => s.id === id);
      return svc && (svc.install_variables.length > 0 || svc.storage_volumes.length > 0);
    });

    if (appsWithConfig.length > 0) {
      setConfigIndex(0);
      setConfigPhase("configure");
    } else {
      // No variables to configure — go straight to summary
      setInstalledApps(
        ids.map(
          (id) => availableApps.find((s) => s.id === id)?.name ?? id,
        ),
      );
      goTo(9 as Step);
    }
  };

  // Apps that need configuration (have install_variables)
  const appsNeedingConfig = Array.from(selectedApps)
    .map((id) => availableApps.find((s) => s.id === id))
    .filter(
      (svc): svc is AvailableApp =>
        !!svc && (svc.install_variables.length > 0 || svc.storage_volumes.length > 0),
    );

  const handleConfigNext = () => {
    if (configIndex < appsNeedingConfig.length - 1) {
      setConfigIndex(configIndex + 1);
    } else {
      // All apps configured — go to summary
      const ids = Array.from(selectedApps);
      setInstalledApps(
        ids.map(
          (id) => availableApps.find((s) => s.id === id)?.name ?? id,
        ),
      );
      setConfigPhase("select");
      goTo(9 as Step);
    }
  };

  const handleConfigBack = () => {
    if (configIndex > 0) {
      setConfigIndex(configIndex - 1);
    } else {
      setConfigPhase("select");
    }
  };

  const handleFinish = async () => {
    const ids = Array.from(selectedApps);
    if (ids.length === 0) {
      onComplete();
      return;
    }

    setDeploying(true);
    setDeployQueue(ids);
    setDeployDone([]);

    // Phase 1: Install all apps sequentially
    for (const id of ids) {
      try {
        const mainPath = allMainPaths[id];
        const hasAdvanced = showAdvancedStorage[id];
        await api.installApp(id, {
          variables: allVariables[id] ?? {},
          ...(mainPath && !hasAdvanced ? { storage_path: mainPath } : {}),
        });
        if (hasAdvanced) {
          const storagePaths = allStoragePaths[id];
          if (storagePaths && Object.keys(storagePaths).length > 0) {
            await api.updateStorage(id, storagePaths);
          }
        }
      } catch {
        // Best-effort: continue with remaining apps
      }
    }

    // Phase 2: Fire all deploys (backend semaphore limits concurrency)
    for (const id of ids) {
      api.deployApp(id).catch(() => {});
    }

    // Phase 3: Poll until all deploys finish
    const pending = new Set(ids);
    while (pending.size > 0) {
      await new Promise((r) => setTimeout(r, 3000));
      try {
        const apps = await api.apps();
        const active: string[] = [];
        for (const app of apps) {
          if (pending.has(app.id)) {
            if (app.deploying) {
              active.push(app.id);
            } else {
              pending.delete(app.id);
              setDeployDone((prev) =>
                prev.includes(app.id) ? prev : [...prev, app.id],
              );
            }
          }
        }
        setDeployActive(active);
      } catch {
        // Keep polling
      }
    }

    setDeploying(false);
  };

  // ── Step indicator ──────────────────────────────────────────────────────

  const StepIndicator = () => (
    <div class="flex items-center justify-center gap-1 mb-8">
      {STEP_LABELS.map((label, i) => {
        const num = (i + 1) as Step;
        const isCurrent = num === step;
        const isComplete = num < step;
        return (
          <div key={num} class="flex items-center">
            {i > 0 && (
              <div
                class={`w-6 h-px mx-1 ${isComplete ? "bg-amber-600/50" : "bg-gray-700"}`}
              />
            )}
            <div class="flex flex-col items-center gap-1">
              <div
                class={`w-7 h-7 rounded-full flex items-center justify-center text-xs font-medium ${
                  isCurrent
                    ? "bg-amber-600 text-white"
                    : isComplete
                      ? "bg-amber-600/20 text-amber-400"
                      : "bg-gray-800 text-gray-500"
                }`}
              >
                {isComplete ? "\u2713" : num}
              </div>
              <span
                class={`text-[10px] ${isCurrent ? "text-amber-400" : isComplete ? "text-gray-500" : "text-gray-600"}`}
              >
                {label}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );

  // ── Render steps ────────────────────────────────────────────────────────

  return (
    <div class="min-h-screen bg-gray-950 flex items-center justify-center p-6">
      <div class="w-full max-w-xl">
        <StepIndicator />

        {/* Step 1: Welcome */}
        {step === 1 && (
          <div class="text-center">
            <h1 class="text-3xl font-bold text-gray-100 mb-3">
              Welcome to MyGround
            </h1>
            <p class="text-gray-400 mb-8">
              Your self-hosted alternative to Google, Apple, and Microsoft
              apps.
            </p>
            <div class="text-left bg-gray-900 rounded-lg p-5 mb-8 space-y-3">
              <p class="text-sm text-gray-300 font-medium">
                In a few steps you'll:
              </p>
              <ol class="list-decimal list-inside space-y-2 text-sm text-gray-400 marker:text-amber-500">
                <li>Create your admin account</li>
                <li>Choose where to store app data</li>
                <li>Access your apps remotely (Tailscale)</li>
                <li>Protect your traffic with a VPN</li>
                <li>Put apps on your own domain (Cloudflare)</li>
                <li>Set up automatic backups</li>
                <li>Pick apps to install</li>
              </ol>
              <p class="text-xs text-gray-500 mt-1">
                Steps 3{"\u20136"} are optional and can be skipped. Each step explains what it does and walks you through getting any accounts or keys you need.
              </p>
            </div>
            <button
              class="px-8 py-3 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded"
              onClick={() => goTo(2)}
            >
              Get Started
            </button>
          </div>
        )}

        {/* Step 2: Account */}
        {step === 2 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Create your account
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              This will be the admin account for your MyGround instance.
            </p>

            <form onSubmit={handleAccountSubmit} class="space-y-4">
              <div>
                <label class="block text-sm font-medium text-gray-300 mb-1">
                  Username
                </label>
                <input
                  type="text"
                  value={username}
                  onInput={(e) =>
                    setUsername((e.target as HTMLInputElement).value)
                  }
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                  placeholder="admin"
                  autoFocus
                />
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-300 mb-1">
                  Password
                </label>
                <input
                  type="password"
                  value={password}
                  onInput={(e) =>
                    setPassword((e.target as HTMLInputElement).value)
                  }
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                />
                <p class="text-xs text-gray-500 mt-1">
                  At least 8 characters
                </p>
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-300 mb-1">
                  Confirm Password
                </label>
                <input
                  type="password"
                  value={confirmPassword}
                  onInput={(e) =>
                    setConfirmPassword((e.target as HTMLInputElement).value)
                  }
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                />
              </div>

              {error && <p class="text-red-400 text-sm">{error}</p>}

              <div class="flex gap-3 pt-2">
                <button
                  type="button"
                  class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                  onClick={() => goTo(1)}
                >
                  Back
                </button>
                <button
                  type="submit"
                  disabled={loading}
                  class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                >
                  {loading ? "Creating account..." : "Create Account"}
                </button>
              </div>
            </form>
          </div>
        )}

        {/* Step 3: Storage */}
        {step === 3 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Storage location
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Where should MyGround store its data? This is the default location for app data like Immich photos, Nextcloud files, etc. You can change individual app paths later.
            </p>

            <div class="bg-gray-900 rounded-lg p-4 mb-4">
              <p class="text-xs text-gray-500 mb-1">Current default</p>
              <p class="text-sm font-mono text-gray-200">{storagePath || "~/.myground/apps/"}</p>
            </div>

            {browsing ? (
              <div class="mb-4">
                <PathPicker
                  initialPath={storagePath || "/"}
                  onSelect={(path) => {
                    setBrowsing(false);
                    handleStorageSave(path);
                  }}
                  onCancel={() => setBrowsing(false)}
                />
              </div>
            ) : (
              <div class="flex gap-3 mb-4">
                <button
                  class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded text-sm"
                  onClick={() => setBrowsing(true)}
                >
                  Choose a different path...
                </button>
              </div>
            )}

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                disabled={loading}
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                onClick={handleStorageSkip}
              >
                {loading ? "Saving..." : "Use Default"}
              </button>
            </div>
          </div>
        )}

        {/* Step 4: Tailscale */}
        {step === 4 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Access your apps from anywhere
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Tailscale lets you securely reach your apps from any device — phone, laptop, or tablet — even when you're away from home.
              It's free for personal use and takes about 2 minutes to set up. You can always do this later in Settings.
            </p>

            <div class="mb-5">
              <TailscaleGuide />
            </div>

            <div class="mb-4">
              <label class="block text-sm font-medium text-gray-300 mb-1">
                Auth Key
              </label>
              <input
                type="text"
                value={tailscaleKey}
                onInput={(e) =>
                  setTailscaleKey((e.target as HTMLInputElement).value)
                }
                class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500 font-mono text-sm"
                placeholder="tskey-auth-..."
              />
            </div>

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(3)}
              >
                Back
              </button>
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(5 as Step)}
              >
                Skip
              </button>
              <button
                disabled={loading}
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                onClick={handleTailscaleEnable}
              >
                {loading ? "Enabling..." : "Enable Tailscale"}
              </button>
            </div>
          </div>
        )}

        {/* Step 5: VPN */}
        {step === 5 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Protect your traffic with a VPN
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              A VPN hides your apps' internet traffic from your ISP and changes their public IP address.
              Useful for torrenting or any app where you want extra privacy.
              If you already have a VPN subscription, enter your credentials below — otherwise, skip this step.
            </p>

            <div class="space-y-4 mb-6">
              <div>
                <label class="block text-sm font-medium text-gray-300 mb-1">Provider</label>
                <select
                  value={vpnProvider}
                  onChange={(e) => {
                    setVpnProvider((e.target as HTMLSelectElement).value);
                    setVpnEnvVars({});
                  }}
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                >
                  <option value="protonvpn">ProtonVPN</option>
                  <option value="nordvpn">NordVPN</option>
                  <option value="mullvad">Mullvad</option>
                  <option value="custom">Custom</option>
                </select>
              </div>
              <div>
                <label class="block text-sm font-medium text-gray-300 mb-1">VPN Type</label>
                <select
                  value={vpnType}
                  onChange={(e) => setVpnType((e.target as HTMLSelectElement).value)}
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
                >
                  <option value="openvpn">OpenVPN</option>
                  <option value="wireguard">WireGuard</option>
                </select>
              </div>
              {vpnType === "openvpn" && (
                <>
                  <p class="text-xs text-gray-500">
                    Find these in your VPN provider's dashboard under "OpenVPN credentials" or "Manual setup".
                  </p>
                  <div>
                    <label class="block text-sm font-medium text-gray-300 mb-1">Username</label>
                    <input
                      type="text"
                      value={vpnEnvVars["OPENVPN_USER"] || ""}
                      onInput={(e) => setVpnEnvVars({ ...vpnEnvVars, OPENVPN_USER: (e.target as HTMLInputElement).value })}
                      class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                    />
                  </div>
                  <div>
                    <label class="block text-sm font-medium text-gray-300 mb-1">Password</label>
                    <input
                      type="password"
                      value={vpnEnvVars["OPENVPN_PASSWORD"] || ""}
                      onInput={(e) => setVpnEnvVars({ ...vpnEnvVars, OPENVPN_PASSWORD: (e.target as HTMLInputElement).value })}
                      class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                    />
                  </div>
                  {vpnProvider === "protonvpn" && (
                    <p class="text-xs text-gray-500">
                      Use your <a href="https://account.protonvpn.com/account#openvpn" target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline">OpenVPN/IKEv2 credentials</a>, not your Proton account password. Required if you have 2FA enabled.
                      {vpnPortForward && " Append +pmp to your username (e.g. user123+pmp) for port forwarding to work."}
                    </p>
                  )}
                </>
              )}
              {vpnType === "wireguard" && (
                <div>
                  <label class="block text-sm font-medium text-gray-300 mb-1">Private Key</label>
                  <p class="text-xs text-gray-500 mb-1">
                    Find this in your VPN provider's dashboard under "WireGuard configuration" or "Manual setup".
                  </p>
                  <input
                    type="password"
                    value={vpnEnvVars["WIREGUARD_PRIVATE_KEY"] || ""}
                    onInput={(e) => setVpnEnvVars({ ...vpnEnvVars, WIREGUARD_PRIVATE_KEY: (e.target as HTMLInputElement).value })}
                    class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                  />
                </div>
              )}
              <div>
                <label class="block text-sm font-medium text-gray-300 mb-1">
                  Server Country (optional) — <a href={{ protonvpn: "https://protonvpn.com/vpn-servers", nordvpn: "https://nordvpn.com/servers/", mullvad: "https://mullvad.net/en/servers", custom: "https://github.com/qdm12/gluetun-wiki/tree/main/setup/providers" }[vpnProvider] || "https://github.com/qdm12/gluetun-wiki/tree/main/setup/providers"} target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline text-xs font-normal">see supported countries</a>
                </label>
                <input
                  type="text"
                  value={vpnCountry}
                  onInput={(e) => setVpnCountry((e.target as HTMLInputElement).value)}
                  placeholder="e.g. Netherlands"
                  class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
                />
              </div>
              <div>
                <label class="flex items-center gap-2 text-sm text-gray-300">
                  <input
                    type="checkbox"
                    checked={vpnPortForward}
                    onChange={(e) => setVpnPortForward((e.target as HTMLInputElement).checked)}
                    class="rounded bg-gray-800 border-gray-600"
                  />
                  Enable port forwarding (recommended)
                </label>
                <p class="text-xs text-gray-500 mt-1">
                  Required for torrent seeding and other apps that need to accept incoming connections.
                  Leave this on unless you know you don't need it — most VPN providers support it at no extra cost.
                </p>
              </div>
            </div>

            <div class="flex items-center gap-3 mb-4">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50"
                disabled={vpnTesting}
                onClick={async () => {
                  setVpnTesting(true);
                  setVpnTestResult(null);
                  setVpnTestLogs([]);
                  setError("");
                  const cfg: VpnConfig = {
                    enabled: true,
                    provider: vpnProvider,
                    vpn_type: vpnType,
                    server_countries: vpnCountry || undefined,
                    port_forwarding: vpnPortForward,
                    env_vars: vpnEnvVars,
                  };
                  const ok = await api.testVpn(cfg, (line) =>
                    setVpnTestLogs((prev) => [...prev, line])
                  );
                  setVpnTestResult(ok);
                  setVpnTesting(false);
                }}
              >
                {vpnTesting ? "Testing..." : "Test Connection"}
              </button>
              {vpnTestResult !== null && (
                <span class={`text-sm ${vpnTestResult ? "text-green-400" : "text-red-400"}`}>
                  {vpnTestResult ? "Connected" : "Failed"}
                </span>
              )}
            </div>
            {vpnTestLogs.length > 0 && (
              <pre class="p-3 bg-gray-950 rounded text-xs text-gray-400 font-mono max-h-48 overflow-y-auto whitespace-pre-wrap mb-4">
                {vpnTestLogs.join("\n")}
              </pre>
            )}

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(4)}
              >
                Back
              </button>
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(6 as Step)}
              >
                Skip
              </button>
              <button
                disabled={loading}
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                onClick={handleVpnEnable}
              >
                {loading ? "Saving..." : "Enable VPN"}
              </button>
            </div>
          </div>
        )}

        {/* Step 6: Cloudflare */}
        {step === 6 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Put apps on your own domain
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Want to reach your apps at addresses like <span class="font-mono text-gray-300">photos.yourdomain.com</span>?
              Cloudflare Tunnel makes this easy — no port forwarding or complicated network setup needed.
              You'll need a free Cloudflare account and a domain. You can always do this later in Settings.
            </p>

            <div class="bg-gray-900 rounded-lg p-4 mb-5 text-sm text-gray-400 space-y-2">
              <p class="text-gray-300 font-medium">How to get an API token:</p>
              <ol class="list-decimal list-inside space-y-1 text-xs">
                <li>
                  Sign up or log in at{" "}
                  <a
                    href="https://dash.cloudflare.com"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="text-amber-400 hover:text-amber-300 underline"
                  >
                    dash.cloudflare.com
                  </a>
                  {" "}and add your domain
                </li>
                <li>
                  Go to{" "}
                  <a
                    href="https://dash.cloudflare.com/profile/api-tokens"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="text-amber-400 hover:text-amber-300 underline"
                  >
                    Profile &gt; API Tokens
                  </a>
                  {" "}and click "Create Token"
                </li>
                <li>Choose "Custom token" and add these permissions:</li>
              </ol>
              <ul class="list-disc list-inside text-xs pl-4 space-y-0.5">
                <li>Account &gt; Cloudflare Tunnel &gt; Edit</li>
                <li>Zone &gt; DNS &gt; Edit</li>
                <li>Account &gt; Account Settings &gt; Read</li>
              </ul>
              <p class="text-xs text-gray-500 mt-1">Copy the token and paste it below.</p>
            </div>

            <div class="mb-4">
              <label class="block text-sm font-medium text-gray-300 mb-1">
                API Token
              </label>
              <input
                type="password"
                value={cloudflareToken}
                onInput={(e) =>
                  setCloudflareToken((e.target as HTMLInputElement).value)
                }
                class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500 font-mono text-sm"
                placeholder="Cloudflare API token"
              />
            </div>

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}
            {cloudflareProgress && (
              <p class="text-amber-400 text-sm mb-4">{cloudflareProgress}</p>
            )}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                disabled={loading}
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded disabled:opacity-50"
                onClick={() => goTo(5)}
              >
                Back
              </button>
              <button
                type="button"
                disabled={loading}
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded disabled:opacity-50"
                onClick={() => goTo(7 as Step)}
              >
                Skip
              </button>
              <button
                disabled={loading}
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                onClick={handleCloudflareEnable}
              >
                {loading ? "Enabling..." : "Enable Cloudflare"}
              </button>
            </div>
          </div>
        )}

        {/* Step 7: Backups */}
        {step === 7 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Set up automatic backups
            </h1>
            <p class="text-gray-400 mb-4 text-sm">
              Keep your data safe by backing up to a local drive or a cloud storage bucket (like AWS S3).
              If a drive fails or something goes wrong, you can restore everything.
              You can always configure this later in Settings.
            </p>

            <div class="flex gap-2 bg-gray-800/50 rounded p-3 mb-6">
              <span class="text-blue-400 shrink-0" aria-hidden="true">&#9432;</span>
              <div class="text-sm text-gray-400 space-y-1">
                <p>
                  All backups are <strong class="text-gray-300">incremental</strong> (only changed data is stored, saving space) and <strong class="text-gray-300">encrypted</strong> (your data is protected even if someone accesses the backup drive or bucket).
                </p>
                <p class="text-amber-400">
                  Your backup password is generated automatically per app. Write it down or store it somewhere safe — without it, backups cannot be restored.
                </p>
              </div>
            </div>

            <div class="space-y-4 mb-6">
              <label class="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={backupLocalEnabled}
                  onChange={(e) =>
                    setBackupLocalEnabled((e.target as HTMLInputElement).checked)
                  }
                  class="rounded bg-gray-700 border-gray-600"
                />
                <span class="text-gray-300">Back up to a local drive</span>
              </label>

              {backupLocalEnabled && (
                <div class="pl-6 space-y-3">
                  <p class="text-xs text-gray-500">
                    Pick a folder on an external or secondary drive. Avoid using the same drive as your app data.
                  </p>
                  <div>
                    <label class="text-xs text-gray-500 block mb-1">
                      Backup folder
                    </label>
                    <div class="flex gap-2 items-center">
                      <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate min-w-0">
                        {backupLocalPath || "/mnt/backups"}
                      </span>
                      <button
                        type="button"
                        class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
                        onClick={() => setBrowsingBackupPath(!browsingBackupPath)}
                      >
                        {browsingBackupPath ? "Cancel" : "Browse"}
                      </button>
                    </div>
                    {browsingBackupPath && (
                      <div class="mt-2">
                        <PathPicker
                          initialPath={backupLocalPath || "/"}
                          onSelect={(path) => {
                            setBackupLocalPath(path);
                            setBrowsingBackupPath(false);
                          }}
                          onCancel={() => setBrowsingBackupPath(false)}
                        />
                      </div>
                    )}
                  </div>
                </div>
              )}

              <label class="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={backupRemoteEnabled}
                  onChange={(e) =>
                    setBackupRemoteEnabled((e.target as HTMLInputElement).checked)
                  }
                  class="rounded bg-gray-700 border-gray-600"
                />
                <span class="text-gray-300">Back up to the cloud (S3-compatible storage)</span>
              </label>

              {backupRemoteEnabled && (
                <div class="pl-6 space-y-4">
                  <AwsSetupForm
                    currentRepository={backupRemoteRepo || undefined}
                    onSuccess={(result: AwsSetupResult) => {
                      setBackupRemoteRepo(result.repository);
                      setBackupS3Key(result.s3_access_key);
                      setBackupS3Secret(result.s3_secret_key);
                    }}
                  />
                  <details class="group">
                    <summary class="text-xs text-gray-500 cursor-pointer hover:text-gray-400">
                      Advanced / Manual setup
                    </summary>
                    <div class="mt-3 space-y-3">
                      <p class="text-xs text-gray-500">
                        Works with AWS S3, Backblaze B2, Cloudflare R2, MinIO, or any S3-compatible service.
                        You'll find these credentials in your storage provider's dashboard.
                      </p>
                      <Field
                        label="Bucket URL"
                        type="text"
                        value={backupRemoteRepo}
                        placeholder="s3:https://s3.amazonaws.com/mybucket"
                        onInput={(v) => setBackupRemoteRepo(v)}
                      />
                      <Field
                        label="Access Key"
                        type="text"
                        value={backupS3Key}
                        onInput={(v) => setBackupS3Key(v)}
                      />
                      <Field
                        label="Secret Key"
                        type="password"
                        value={backupS3Secret}
                        onInput={(v) => setBackupS3Secret(v)}
                      />
                    </div>
                  </details>
                </div>
              )}
            </div>

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(6)}
              >
                Back
              </button>
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(8 as Step)}
              >
                Skip
              </button>
              <button
                disabled={loading || (!backupLocalEnabled && !backupRemoteEnabled)}
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                onClick={handleBackupSave}
              >
                {loading ? "Saving..." : "Save"}
              </button>
            </div>
          </div>
        )}

        {/* Step 8: Apps — selection or configure carousel */}
        {step === 8 && configPhase === "select" && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Pick apps to install
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Select apps to set up with sensible defaults. You can always
              add more later.
            </p>

            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-6 max-h-80 overflow-y-auto pr-1">
              {availableApps.map((svc) => (
                <button
                  key={svc.id}
                  type="button"
                  class={`text-left p-3 rounded-lg border transition-colors ${
                    selectedApps.has(svc.id)
                      ? "border-amber-600 bg-amber-600/10"
                      : "border-gray-700 bg-gray-900 hover:border-gray-600"
                  }`}
                  onClick={() => toggleApp(svc.id)}
                >
                  <div class="flex items-start gap-3">
                    <AppIcon id={svc.id} class="w-6 h-6 shrink-0" />
                    <div class="min-w-0">
                      <p class="text-sm font-medium text-gray-200 truncate">
                        {svc.name}
                      </p>
                      <p class="text-xs text-gray-500">
                        {svc.description}
                      </p>
                    </div>
                    <div class="ml-auto shrink-0">
                      <div
                        class={`w-5 h-5 rounded border flex items-center justify-center ${
                          selectedApps.has(svc.id)
                            ? "border-amber-500 bg-amber-600 text-white"
                            : "border-gray-600"
                        }`}
                      >
                        {selectedApps.has(svc.id) && (
                          <span class="text-xs">{"\u2713"}</span>
                        )}
                      </div>
                    </div>
                  </div>
                </button>
              ))}
            </div>

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(7 as Step)}
              >
                Back
              </button>
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(9 as Step)}
              >
                Skip
              </button>
              <button
                disabled={selectedApps.size === 0}
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                onClick={handleNextFromSelection}
              >
                Next ({selectedApps.size})
              </button>
            </div>
          </div>
        )}

        {/* Step 8: Configure variables carousel */}
        {step === 8 && configPhase === "configure" && appsNeedingConfig[configIndex] && (
          <div>
            <div class="flex items-center gap-3 mb-1">
              <AppIcon
                id={appsNeedingConfig[configIndex].id}
                class="w-8 h-8"
              />
              <h1 class="text-2xl font-bold text-gray-100">
                Configure {appsNeedingConfig[configIndex].name}
              </h1>
            </div>
            <p class="text-gray-500 text-xs mb-5">
              App {configIndex + 1} of {appsNeedingConfig.length}
            </p>

            <div class="space-y-4 mb-6">
              {appsNeedingConfig[configIndex].install_variables.map((v) => (
                <VariableField
                  key={v.key}
                  variable={v}
                  value={
                    allVariables[appsNeedingConfig[configIndex].id]?.[v.key] ??
                    ""
                  }
                  onChange={(key, val) =>
                    setAllVariables((prev) => ({
                      ...prev,
                      [appsNeedingConfig[configIndex].id]: {
                        ...prev[appsNeedingConfig[configIndex].id],
                        [key]: val,
                      },
                    }))
                  }
                />
              ))}

              {appsNeedingConfig[configIndex].storage_volumes.length > 0 && (() => {
                const appId = appsNeedingConfig[configIndex].id;
                const mainPath = allMainPaths[appId] ?? "";
                const isAdvanced = showAdvancedStorage[appId] ?? false;
                const hasMultipleVolumes = appsNeedingConfig[configIndex].storage_volumes.length > 1;
                return (
                <div>
                  {appsNeedingConfig[configIndex].install_variables.length > 0 && (
                    <hr class="border-gray-800 my-4" />
                  )}
                  <p class="text-sm font-medium text-gray-300 mb-3">Storage</p>
                  <div class={isAdvanced ? "opacity-40 pointer-events-none" : ""}>
                    {browsingMainPath === appId ? (
                      <PathPicker
                        initialPath={mainPath || "/"}
                        onSelect={(path) => {
                          setBrowsingMainPath(null);
                          setAllMainPaths((prev) => ({ ...prev, [appId]: path }));
                          // Update per-volume defaults based on new main path
                          const vols = appsNeedingConfig[configIndex].storage_volumes;
                          const volPaths: Record<string, string> = {};
                          for (const vol of vols) {
                            volPaths[vol.name] = vols.length <= 1
                              ? `${path}/`
                              : `${path}/${vol.name}/`;
                          }
                          setAllStoragePaths((prev) => ({ ...prev, [appId]: volPaths }));
                        }}
                        onCancel={() => setBrowsingMainPath(null)}
                      />
                    ) : (
                      <div class="flex items-center gap-2">
                        <p class="text-sm font-mono text-gray-200 truncate flex-1">
                          {mainPath}
                        </p>
                        <button
                          type="button"
                          class="px-2 py-1 text-xs bg-gray-700 hover:bg-gray-600 text-gray-300 rounded shrink-0"
                          onClick={() => setBrowsingMainPath(appId)}
                        >
                          Change
                        </button>
                      </div>
                    )}
                  </div>
                  {hasMultipleVolumes && (
                    <div class="pt-2 border-t border-gray-800 mt-3">
                      <button
                        type="button"
                        class="text-xs text-gray-500 hover:text-gray-300"
                        onClick={() => setShowAdvancedStorage((prev) => ({ ...prev, [appId]: !prev[appId] }))}
                      >
                        {isAdvanced ? "Hide" : "Advanced"}: per-volume paths
                      </button>
                      {isAdvanced && (
                        <div class="mt-3 space-y-3">
                          <p class="text-xs text-amber-400">
                            Per-volume paths override the main storage path.
                          </p>
                          {appsNeedingConfig[configIndex].storage_volumes.map((vol) => {
                            const currentPath = allStoragePaths[appId]?.[vol.name] ?? "";
                            const isBrowsingThis = browsingVolume?.appId === appId && browsingVolume?.volName === vol.name;
                            return (
                              <div key={vol.name} class="bg-gray-800 rounded-lg p-3">
                                <p class="text-xs text-gray-400 mb-1">{vol.description}</p>
                                {isBrowsingThis ? (
                                  <PathPicker
                                    initialPath={currentPath || "/"}
                                    onSelect={(path) => {
                                      setBrowsingVolume(null);
                                      setAllStoragePaths((prev) => ({
                                        ...prev,
                                        [appId]: { ...prev[appId], [vol.name]: path },
                                      }));
                                    }}
                                    onCancel={() => setBrowsingVolume(null)}
                                  />
                                ) : (
                                  <div class="flex items-center gap-2">
                                    <p class="text-xs font-mono text-gray-300 truncate flex-1">
                                      {currentPath}
                                    </p>
                                    <button
                                      type="button"
                                      class="px-2 py-1 text-xs bg-gray-700 hover:bg-gray-600 text-gray-300 rounded shrink-0"
                                      onClick={() => setBrowsingVolume({ appId, volName: vol.name })}
                                    >
                                      Change
                                    </button>
                                  </div>
                                )}
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  )}
                </div>
                );
              })()}
            </div>

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={handleConfigBack}
              >
                Back
              </button>
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={handleConfigNext}
              >
                Use Defaults
              </button>
              <button
                class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded"
                onClick={handleConfigNext}
              >
                {configIndex < appsNeedingConfig.length - 1 ? "Next" : "Done"}
              </button>
            </div>
          </div>
        )}

        {/* Step 9: Done */}
        {step === 9 && (
          <div class="text-center">
            <h1 class="text-3xl font-bold text-gray-100 mb-3">
              You're all set!
            </h1>
            <p class="text-gray-400 mb-8">
              Your MyGround instance is ready to use.
            </p>

            <div class="text-left bg-gray-900 rounded-lg p-5 mb-8 space-y-3">
              <div class="flex items-center gap-3 text-sm">
                <span class="text-green-400">{"\u2713"}</span>
                <span class="text-gray-300">
                  Account created as <strong>{username}</strong>
                </span>
              </div>
              {configuredStorage && (
                <div class="flex items-center gap-3 text-sm">
                  <span class="text-green-400">{"\u2713"}</span>
                  <span class="text-gray-300">
                    Storage:{" "}
                    <span class="font-mono text-gray-400">
                      {configuredStorage}
                    </span>
                  </span>
                </div>
              )}
              <div class="flex items-center gap-3 text-sm">
                <span
                  class={
                    configuredTailscale ? "text-green-400" : "text-gray-600"
                  }
                >
                  {configuredTailscale ? "\u2713" : "\u2013"}
                </span>
                <span class="text-gray-300">
                  Tailscale{" "}
                  {configuredTailscale ? "enabled" : "not configured (you can enable it later)"}
                </span>
              </div>
              <div class="flex items-center gap-3 text-sm">
                <span
                  class={
                    configuredVpn ? "text-green-400" : "text-gray-600"
                  }
                >
                  {configuredVpn ? "\u2713" : "\u2013"}
                </span>
                <span class="text-gray-300">
                  VPN{" "}
                  {configuredVpn ? "enabled" : "not configured (you can enable it later)"}
                </span>
              </div>
              <div class="flex items-center gap-3 text-sm">
                <span
                  class={
                    configuredCloudflare ? "text-green-400" : "text-gray-600"
                  }
                >
                  {configuredCloudflare ? "\u2713" : "\u2013"}
                </span>
                <span class="text-gray-300">
                  Cloudflare Tunnel{" "}
                  {configuredCloudflare ? "enabled" : "not configured (you can enable it later)"}
                </span>
              </div>
              <div class="flex items-center gap-3 text-sm">
                <span
                  class={
                    configuredBackup ? "text-green-400" : "text-gray-600"
                  }
                >
                  {configuredBackup ? "\u2713" : "\u2013"}
                </span>
                <span class="text-gray-300">
                  {configuredBackup
                    ? `Backups: ${backupLocalEnabled ? `Local (${backupLocalPath || "/mnt/backups"})` : ""}${backupLocalEnabled && backupRemoteEnabled ? " + " : ""}${backupRemoteEnabled ? "S3" : ""}`
                    : "Backups not configured (you can enable it later)"}
                </span>
              </div>
              {installedApps.length > 0 && (
                <div class="flex items-start gap-3 text-sm">
                  <span class="text-green-400 mt-0.5">{"\u2713"}</span>
                  <span class="text-gray-300">
                    Installed: {installedApps.join(", ")}
                  </span>
                </div>
              )}
              {installedApps.length === 0 && (
                <div class="flex items-center gap-3 text-sm">
                  <span class="text-gray-600">{"\u2013"}</span>
                  <span class="text-gray-300">
                    No apps installed (add them from the dashboard)
                  </span>
                </div>
              )}
            </div>

            {(deploying || deployDone.length > 0) && deployQueue.length > 0 && (
              <div class="bg-gray-900 rounded-lg p-4 mb-6">
                <p class="text-sm text-gray-300 mb-3 font-medium">
                  {deploying
                    ? `Deploying apps (${deployDone.length}/${deployQueue.length})...`
                    : `All ${deployQueue.length} apps deployed`}
                </p>
                <div class="space-y-1.5">
                  {deployQueue.map((id) => {
                    const done = deployDone.includes(id);
                    const active = deployActive.includes(id);
                    return (
                      <div key={id} class="flex items-center gap-2 text-sm">
                        <span class={done ? "text-green-400" : active ? "text-amber-400" : "text-gray-600"}>
                          {done ? "\u2713" : active ? "\u25CF" : "\u25CB"}
                        </span>
                        <span class={done ? "text-gray-400" : "text-gray-300"}>
                          {id}
                        </span>
                        {!done && deploying && (
                          <span class={`text-xs ${active ? "text-amber-400" : "text-gray-500"}`}>
                            {active ? "deploying..." : "pending"}
                          </span>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            <button
              class="px-8 py-3 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
              onClick={deploying ? undefined : deployQueue.length > 0 ? onComplete : handleFinish}
              disabled={deploying}
            >
              {deploying ? "Deploying..." : deployQueue.length > 0 ? "Go to Dashboard" : "Finish Setup"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
