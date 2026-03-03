import { useState } from "preact/hooks";
import {
  api,
  generatePassword,
  type AvailableApp,
  type GlobalConfig,
  type VpnConfig,
} from "../api";
import { PathPicker } from "../components/path-picker";
import { AppIcon } from "../components/app-icon";
import { TailscaleGuide } from "../components/tailscale-guide";
import { VariableField } from "../components/variable-field";

interface Props {
  onComplete: () => void;
}

type Step = 1 | 2 | 3 | 4 | 5 | 6 | 7;

const STEP_LABELS = [
  "Welcome",
  "Account",
  "Storage",
  "Tailscale",
  "VPN",
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

  // Step 6: Apps
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

  // Step 7: Summary
  const [configuredStorage, setConfiguredStorage] = useState<string | null>(
    null,
  );
  const [configuredTailscale, setConfiguredTailscale] = useState(false);
  const [configuredVpn, setConfiguredVpn] = useState(false);
  const [installedApps, setInstalledApps] = useState<string[]>([]);

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

  // ── Step 6: Install apps ────────────────────────────────────────────────

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
      goTo(7 as Step);
      return;
    }

    // Pre-populate allVariables with defaults for every selected app
    const vars: Record<string, Record<string, string>> = {};
    const paths: Record<string, Record<string, string>> = {};
    for (const id of ids) {
      const svc = availableApps.find((s) => s.id === id);
      const variables: Record<string, string> = {};
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
      goTo(7 as Step);
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
      goTo(7 as Step);
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
    // Install all selected apps, then trigger deploy for each
    const ids = Array.from(selectedApps);
    for (const id of ids) {
      try {
        await api.installApp(id, { variables: allVariables[id] ?? {} });
        // Update per-volume storage paths if customized
        const storagePaths = allStoragePaths[id];
        if (storagePaths && Object.keys(storagePaths).length > 0) {
          await api.updateStorage(id, storagePaths);
        }
        // Fire-and-forget: trigger background deploy (pull + start)
        api.deployApp(id).catch(() => {});
      } catch {
        // Best-effort: continue with remaining apps
      }
    }
    onComplete();
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
            <div class="text-left bg-gray-900 rounded-lg p-5 mb-8 space-y-2">
              <p class="text-sm text-gray-300 font-medium mb-3">
                In a few steps you'll:
              </p>
              <ol class="list-decimal list-inside space-y-2 text-sm text-gray-400 marker:text-amber-500">
                <li>Create your admin account</li>
                <li>Choose where to store app data</li>
                <li>Optionally enable Tailscale for remote access</li>
                <li>Pick apps to install</li>
              </ol>
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
              Tailscale gives every app its own HTTPS domain on your private
              network. You can set this up later in Settings.
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
              Route app traffic through a VPN
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Optional. Configure your VPN provider — apps can use it with a single toggle.
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
                </>
              )}
              {vpnType === "wireguard" && (
                <div>
                  <label class="block text-sm font-medium text-gray-300 mb-1">Private Key</label>
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
                  Enable port forwarding
                </label>
                <p class="text-xs text-gray-500 mt-1">
                  Requests an open inbound port from the VPN provider for incoming connections
                </p>
              </div>
            </div>

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

        {/* Step 6: Apps — selection or configure carousel */}
        {step === 6 && configPhase === "select" && (
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
                onClick={() => goTo(5 as Step)}
              >
                Back
              </button>
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={() => goTo(7 as Step)}
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

        {/* Step 6: Configure variables carousel */}
        {step === 6 && configPhase === "configure" && appsNeedingConfig[configIndex] && (
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

              {appsNeedingConfig[configIndex].storage_volumes.length > 0 && (
                <div>
                  {appsNeedingConfig[configIndex].install_variables.length > 0 && (
                    <hr class="border-gray-800 my-4" />
                  )}
                  <p class="text-sm font-medium text-gray-300 mb-3">Storage</p>
                  <div class="space-y-3">
                    {appsNeedingConfig[configIndex].storage_volumes.map((vol) => {
                      const appId = appsNeedingConfig[configIndex].id;
                      const currentPath = allStoragePaths[appId]?.[vol.name] ?? "";
                      const isBrowsingThis = browsingVolume?.appId === appId && browsingVolume?.volName === vol.name;
                      return (
                        <div key={vol.name} class="bg-gray-900 rounded-lg p-3">
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
                              <p class="text-sm font-mono text-gray-200 truncate flex-1">
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
                </div>
              )}
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

        {/* Step 7: Done */}
        {step === 7 && (
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

            <button
              class="px-8 py-3 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded"
              onClick={handleFinish}
            >
              Go to Dashboard
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
