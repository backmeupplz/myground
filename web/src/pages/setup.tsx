import { useState } from "preact/hooks";
import {
  api,
  generatePassword,
  type AvailableService,
  type GlobalConfig,
} from "../api";
import { PathPicker } from "../components/path-picker";
import { TailscaleGuide } from "../components/tailscale-guide";

interface Props {
  onComplete: () => void;
}

type Step = 1 | 2 | 3 | 4 | 5 | 6;

const STEP_LABELS = [
  "Welcome",
  "Account",
  "Storage",
  "Tailscale",
  "Services",
  "Done",
];

interface InstallProgress {
  current: number;
  total: number;
  currentName: string;
}

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

  // Step 5: Services
  const [availableServices, setAvailableServices] = useState<
    AvailableService[]
  >([]);
  const [selectedServices, setSelectedServices] = useState<Set<string>>(
    new Set(),
  );
  const [installing, setInstalling] = useState(false);
  const [installProgress, setInstallProgress] =
    useState<InstallProgress | null>(null);

  // Step 6: Summary
  const [configuredStorage, setConfiguredStorage] = useState<string | null>(
    null,
  );
  const [configuredTailscale, setConfiguredTailscale] = useState(false);
  const [installedServices, setInstalledServices] = useState<string[]>([]);

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
      // Pre-fetch available services for step 5
      const services = await api.availableServices();
      setAvailableServices(services);
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
      goTo(5);
    } catch (err: unknown) {
      setError(
        err instanceof Error ? err.message : "Failed to enable Tailscale",
      );
    } finally {
      setLoading(false);
    }
  };

  // ── Step 5: Install services ────────────────────────────────────────────

  const toggleService = (id: string) => {
    setSelectedServices((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const handleInstallServices = async () => {
    const ids = Array.from(selectedServices);
    if (ids.length === 0) {
      goTo(6);
      return;
    }

    setInstalling(true);
    setError("");
    const names: string[] = [];

    for (let i = 0; i < ids.length; i++) {
      const svc = availableServices.find((s) => s.id === ids[i]);
      const name = svc?.name ?? ids[i];
      setInstallProgress({ current: i + 1, total: ids.length, currentName: name });

      // Build default variables
      const variables: Record<string, string> = {};
      if (svc) {
        for (const v of svc.install_variables) {
          if (v.input_type === "password") {
            variables[v.key] = generatePassword(25);
          } else if (v.default) {
            variables[v.key] = v.default;
          }
        }
      }

      try {
        await api.installService(ids[i], { variables });
        names.push(name);
      } catch {
        // Continue with remaining services even if one fails
      }
    }

    setInstalledServices(names);
    setInstalling(false);
    setInstallProgress(null);
    goTo(6);
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
              services.
            </p>
            <div class="text-left bg-gray-900 rounded-lg p-5 mb-8 space-y-2">
              <p class="text-sm text-gray-300 font-medium mb-3">
                In a few steps you'll:
              </p>
              <div class="flex items-start gap-3 text-sm text-gray-400">
                <span class="text-amber-500 mt-0.5 shrink-0">1.</span>
                <span>Create your admin account</span>
              </div>
              <div class="flex items-start gap-3 text-sm text-gray-400">
                <span class="text-amber-500 mt-0.5 shrink-0">2.</span>
                <span>Choose where to store service data</span>
              </div>
              <div class="flex items-start gap-3 text-sm text-gray-400">
                <span class="text-amber-500 mt-0.5 shrink-0">3.</span>
                <span>Optionally enable Tailscale for remote access</span>
              </div>
              <div class="flex items-start gap-3 text-sm text-gray-400">
                <span class="text-amber-500 mt-0.5 shrink-0">4.</span>
                <span>Pick services to install</span>
              </div>
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
              Where should MyGround store service data?
            </p>

            {storagePath && (
              <div class="bg-gray-900 rounded-lg p-4 mb-4">
                <p class="text-xs text-gray-500 mb-1">Current default</p>
                <p class="text-sm font-mono text-gray-200">{storagePath}</p>
              </div>
            )}

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
                  Browse...
                </button>
              </div>
            )}

            {error && <p class="text-red-400 text-sm mb-4">{error}</p>}

            <div class="flex gap-3 pt-2">
              <button
                type="button"
                class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                onClick={handleStorageSkip}
              >
                Use Default
              </button>
              {!browsing && (
                <button
                  disabled={loading}
                  class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                  onClick={() => handleStorageSave(storagePath)}
                >
                  {loading ? "Saving..." : "Confirm Path"}
                </button>
              )}
            </div>
          </div>
        )}

        {/* Step 4: Tailscale */}
        {step === 4 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Access your services from anywhere
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Tailscale gives every service its own HTTPS domain on your private
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
                onClick={() => goTo(5)}
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

        {/* Step 5: Services */}
        {step === 5 && (
          <div>
            <h1 class="text-2xl font-bold text-gray-100 mb-2">
              Pick services to install
            </h1>
            <p class="text-gray-400 mb-6 text-sm">
              Select services to set up with sensible defaults. You can always
              add more later.
            </p>

            {installing ? (
              <div class="space-y-4">
                {installProgress && (
                  <div>
                    <p class="text-sm text-gray-300 mb-2">
                      Installing {installProgress.currentName} (
                      {installProgress.current}/{installProgress.total})...
                    </p>
                    <div class="w-full bg-gray-800 rounded-full h-2">
                      <div
                        class="bg-amber-600 h-2 rounded-full transition-all duration-300"
                        style={{
                          width: `${(installProgress.current / installProgress.total) * 100}%`,
                        }}
                      />
                    </div>
                  </div>
                )}
              </div>
            ) : (
              <>
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-6 max-h-80 overflow-y-auto pr-1">
                  {availableServices.map((svc) => (
                    <button
                      key={svc.id}
                      type="button"
                      class={`text-left p-3 rounded-lg border transition-colors ${
                        selectedServices.has(svc.id)
                          ? "border-amber-600 bg-amber-600/10"
                          : "border-gray-700 bg-gray-900 hover:border-gray-600"
                      }`}
                      onClick={() => toggleService(svc.id)}
                    >
                      <div class="flex items-start gap-3">
                        <span class="text-xl shrink-0">{svc.icon}</span>
                        <div class="min-w-0">
                          <p class="text-sm font-medium text-gray-200 truncate">
                            {svc.name}
                          </p>
                          <p class="text-xs text-gray-500 line-clamp-2">
                            {svc.description}
                          </p>
                        </div>
                        <div class="ml-auto shrink-0">
                          <div
                            class={`w-5 h-5 rounded border flex items-center justify-center ${
                              selectedServices.has(svc.id)
                                ? "border-amber-500 bg-amber-600 text-white"
                                : "border-gray-600"
                            }`}
                          >
                            {selectedServices.has(svc.id) && (
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
                    onClick={() => goTo(4)}
                  >
                    Back
                  </button>
                  <button
                    type="button"
                    class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                    onClick={() => goTo(6)}
                  >
                    Skip
                  </button>
                  <button
                    disabled={selectedServices.size === 0}
                    class="flex-1 py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
                    onClick={handleInstallServices}
                  >
                    Install Selected ({selectedServices.size})
                  </button>
                </div>
              </>
            )}
          </div>
        )}

        {/* Step 6: Done */}
        {step === 6 && (
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
              {installedServices.length > 0 && (
                <div class="flex items-start gap-3 text-sm">
                  <span class="text-green-400 mt-0.5">{"\u2713"}</span>
                  <span class="text-gray-300">
                    Installed: {installedServices.join(", ")}
                  </span>
                </div>
              )}
              {installedServices.length === 0 && (
                <div class="flex items-center gap-3 text-sm">
                  <span class="text-gray-600">{"\u2013"}</span>
                  <span class="text-gray-300">
                    No services installed (add them from the dashboard)
                  </span>
                </div>
              )}
            </div>

            <button
              class="px-8 py-3 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded"
              onClick={onComplete}
            >
              Go to Dashboard
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
