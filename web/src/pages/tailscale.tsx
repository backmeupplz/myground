import { useState, useCallback } from "preact/hooks";
import { api, type TailscaleStatus } from "../api";
import { usePolling } from "../hooks/use-polling";
import { TailscaleGuide } from "../components/tailscale-guide";

export function Tailscale() {
  const fetcher = useCallback(() => api.tailscaleStatus(), []);
  const [status, loading, refetch] = usePolling<TailscaleStatus>(fetcher, 10000);
  const [authKey, setAuthKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");

  const handleSave = async () => {
    setError("");
    setSaving(true);
    try {
      await api.saveTailscaleConfig({
        enabled: true,
        auth_key: authKey.trim() || undefined,
      });
      setAuthKey("");
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleDisable = async () => {
    setError("");
    setSaving(true);
    try {
      await api.saveTailscaleConfig({ enabled: false });
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to disable");
    } finally {
      setSaving(false);
    }
  };

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await api.tailscaleRefresh();
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Refresh failed");
    } finally {
      setRefreshing(false);
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
    <div class="flex-1 px-6 py-6 max-w-4xl mx-auto w-full space-y-6">
      <h1 class="text-xl font-bold">Tailscale</h1>
      <p class="text-gray-400">
        Remote access to your services via Tailscale. Each service gets its own
        HTTPS domain on your tailnet.
      </p>

      {/* Status */}
      <section class="bg-gray-900 rounded-lg p-5 space-y-3">
        <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
          Status
        </h2>
        <div class="flex items-center gap-4 text-sm">
          <span class="text-gray-300">
            Tailscale:{" "}
            <span
              class={
                status?.enabled ? "text-green-400" : "text-gray-500"
              }
            >
              {status?.enabled ? "Enabled" : "Disabled"}
            </span>
          </span>
          {status?.enabled && (
            <span class="text-gray-300">
              TSDProxy:{" "}
              <span
                class={
                  status?.running ? "text-green-400" : "text-yellow-400"
                }
              >
                {status?.running ? "Running" : "Stopped"}
              </span>
            </span>
          )}
          {status?.tailnet && (
            <span class="text-gray-300">
              Tailnet:{" "}
              <span class="text-gray-100 font-mono">{status.tailnet}</span>
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
            <TailscaleGuide />
            <div class="flex gap-2 mt-1">
              <input
                type="text"
                value={authKey}
                onInput={(e) =>
                  setAuthKey((e.target as HTMLInputElement).value)
                }
                placeholder="tskey-auth-..."
                class="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 font-mono text-sm focus:outline-none focus:border-gray-500"
              />
              <button
                onClick={handleSave}
                disabled={saving || !authKey.trim()}
                class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white text-sm rounded disabled:opacity-50"
              >
                {saving ? "Enabling..." : "Enable"}
              </button>
            </div>
          </>
        ) : (
          <div class="flex items-center justify-between">
            <p class="text-sm text-gray-300">
              Tailscale is enabled. Services will be accessible on your tailnet.
            </p>
            <button
              onClick={handleDisable}
              disabled={saving}
              class="px-4 py-2 bg-red-600/80 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50"
            >
              {saving ? "Disabling..." : "Disable"}
            </button>
          </div>
        )}

        {error && <p class="text-red-400 text-sm">{error}</p>}
      </section>

      {/* Per-service list */}
      {status?.enabled && status.services.length > 0 && (
        <section class="bg-gray-900 rounded-lg p-5 space-y-4">
          <div class="flex items-center justify-between">
            <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
              Services on Tailnet
            </h2>
            <button
              onClick={handleRefresh}
              disabled={refreshing}
              class="px-3 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded disabled:opacity-50"
            >
              {refreshing ? "Refreshing..." : "Refresh All"}
            </button>
          </div>
          <div class="space-y-2">
            {status.services.map((svc) => (
              <div
                key={svc.service_id}
                class="flex items-center justify-between py-2 px-3 bg-gray-800 rounded"
              >
                <span class="text-gray-200 font-medium">
                  {svc.service_id}
                </span>
                {svc.url ? (
                  <a
                    href={svc.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    class="text-amber-400 hover:text-amber-300 text-sm font-mono underline"
                  >
                    {svc.url}
                  </a>
                ) : (
                  <span class="text-gray-500 text-sm">
                    Tailnet not detected yet
                  </span>
                )}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
