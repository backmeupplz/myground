import { useState, useCallback } from "preact/hooks";
import { api, type CloudflareStatus } from "../api";
import { usePolling } from "../hooks/use-polling";

function CloudflareGuide() {
  return (
    <div class="text-sm text-gray-400 space-y-2 mb-3">
      <p>
        Create a Cloudflare API token at{" "}
        <a
          href="https://dash.cloudflare.com/profile/api-tokens"
          target="_blank"
          rel="noopener noreferrer"
          class="text-amber-400 hover:text-amber-300 underline"
        >
          dash.cloudflare.com/profile/api-tokens
        </a>{" "}
        with these permissions:
      </p>
      <ul class="list-disc list-inside text-gray-500 space-y-1">
        <li>Account &gt; Cloudflare Tunnel &gt; Edit</li>
        <li>Zone &gt; DNS &gt; Edit</li>
        <li>Account &gt; Account Settings &gt; Read</li>
      </ul>
    </div>
  );
}

const CF_WARNING_KEY = "myground-cloudflare-security-dismissed";

export function Cloudflare(_props: { path?: string }) {
  const fetcher = useCallback(() => api.cloudflareStatus(), []);
  const [status, loading, refetch] = usePolling<CloudflareStatus>(fetcher, 10000);
  const [apiToken, setApiToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [progress, setProgress] = useState("");
  const [warningDismissed, setWarningDismissed] = useState(() => {
    try {
      return globalThis.localStorage?.getItem(CF_WARNING_KEY) === "1";
    } catch {
      return false;
    }
  });

  const handleEnable = async () => {
    setError("");
    setSaving(true);
    setProgress("Validating API token...");
    try {
      setProgress("Creating Cloudflare Tunnel (this may take a minute)...");
      await api.saveCloudflareConfig({
        enabled: true,
        api_token: apiToken.trim(),
      });
      setProgress("Verifying tunnel is running...");
      setApiToken("");
      refetch();
      setProgress("");
    } catch (e: unknown) {
      setProgress("");
      setError(e instanceof Error ? e.message : "Failed to enable");
    } finally {
      setSaving(false);
    }
  };

  const handleDisable = async () => {
    setError("");
    setSaving(true);
    try {
      await api.saveCloudflareConfig({ enabled: false });
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to disable");
    } finally {
      setSaving(false);
    }
  };

  const handleUnbind = async (appId: string) => {
    try {
      await api.unbindDomain(appId);
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to unbind");
    }
  };

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  return (
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full space-y-4 sm:space-y-6">
      <h1 class="text-xl font-bold">Cloudflare</h1>
      <p class="text-gray-400">
        Expose apps on custom domains via Cloudflare Tunnels. Each app
        can be bound to a subdomain like photos.yourdomain.com.
      </p>

      {/* Security warning */}
      {!warningDismissed && (
        <section class="bg-amber-900/20 border border-amber-500/30 rounded-lg p-4 flex gap-3">
          <span class="text-amber-400 shrink-0 text-lg">&#9888;</span>
          <div class="flex-1 space-y-2">
            <p class="text-sm font-medium text-amber-300">
              Security notice
            </p>
            <p class="text-sm text-gray-400">
              Enabling a Cloudflare Tunnel makes your apps accessible from the
              public internet. Anyone with the URL can reach them. MyGround
              generates strong passwords by default, so this is usually fine.
              Just make sure you haven't changed any app passwords to something
              weak, and consider limiting exposure to only the apps you
              actually need to share.
            </p>
          </div>
          <button
            class="p-1 text-gray-400 hover:text-gray-200 shrink-0 self-start"
            onClick={() => {
              try { globalThis.localStorage?.setItem(CF_WARNING_KEY, "1"); } catch {/* */}
              setWarningDismissed(true);
            }}
            title="Dismiss"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
              <path d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </section>
      )}

      {/* Status */}
      <section class="bg-gray-900 rounded-lg p-5 space-y-3">
        <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
          Status
        </h2>
        <div class="flex items-center gap-4 text-sm flex-wrap">
          <span class="text-gray-300">
            Cloudflare:{" "}
            <span
              class={status?.enabled ? "text-green-400" : "text-gray-500"}
            >
              {status?.enabled ? "Connected" : "Disabled"}
            </span>
          </span>
          {status?.enabled && (
            <span class="text-gray-300">
              Tunnel:{" "}
              <span
                class={
                  status?.tunnel_running
                    ? "text-green-400"
                    : "text-yellow-400"
                }
              >
                {status?.tunnel_running ? "Running" : "Stopped"}
              </span>
            </span>
          )}
          {status?.tunnel_id && (
            <span class="text-gray-300">
              Tunnel ID:{" "}
              <span class="text-gray-100 font-mono text-xs">
                {status.tunnel_id}
              </span>
            </span>
          )}
        </div>
      </section>

      {/* Configuration */}
      <section class="bg-gray-900 rounded-lg p-5 space-y-4">
        <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
          Configuration
        </h2>

        {!status?.enabled ? (
          <>
            <CloudflareGuide />
            <div class="flex gap-2 mt-1">
              <input
                type="password"
                value={apiToken}
                onInput={(e) =>
                  setApiToken((e.target as HTMLInputElement).value)
                }
                placeholder="Cloudflare API token"
                class="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 font-mono text-sm focus:outline-none focus:border-gray-500"
              />
              <button
                onClick={handleEnable}
                disabled={saving || !apiToken.trim()}
                class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white text-sm rounded disabled:opacity-50"
              >
                {saving ? "Connecting..." : "Enable"}
              </button>
            </div>
          </>
        ) : (
          <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
            <p class="text-sm text-gray-300">
              Cloudflare is connected. Bind domains to apps from their
              detail pages.
            </p>
            <button
              onClick={handleDisable}
              disabled={saving}
              class="px-4 py-2 bg-red-600/80 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50 shrink-0 self-start sm:self-auto"
            >
              {saving ? "Disabling..." : "Disable"}
            </button>
          </div>
        )}

        {progress && <p class="text-amber-400 text-sm">{progress}</p>}
        {error && <p class="text-red-400 text-sm">{error}</p>}
      </section>

      {/* Domain bindings table */}
      {status?.enabled && (
        <section class="bg-gray-900 rounded-lg p-5 space-y-4">
          <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
            Domain Bindings
          </h2>
          {status.bindings.length > 0 ? (
            <div class="space-y-2">
              {status.bindings.map((b) => (
                <div
                  key={b.app_id}
                  class="flex flex-col sm:flex-row sm:items-center justify-between gap-2 py-2 px-3 bg-gray-800 rounded"
                >
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-gray-200 font-medium shrink-0">
                      {b.app_name}
                    </span>
                    <a
                      href={`https://${b.fqdn}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      class="text-amber-400 hover:text-amber-300 text-sm font-mono underline truncate"
                    >
                      {b.fqdn}
                    </a>
                  </div>
                  <button
                    onClick={() => handleUnbind(b.app_id)}
                    class="px-3 py-1 bg-gray-600 hover:bg-gray-500 text-gray-200 text-xs rounded shrink-0 self-start sm:self-auto"
                  >
                    Unbind
                  </button>
                </div>
              ))}
            </div>
          ) : (
            <p class="text-sm text-gray-500">
              No domains bound yet. Go to an app's detail page to bind a
              domain.
            </p>
          )}
        </section>
      )}
    </div>
  );
}
