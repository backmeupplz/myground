import { useState, useCallback, useRef, useEffect } from "preact/hooks";
import { api, type TailscaleStatus } from "../api";
import { usePolling } from "../hooks/use-polling";
import { TailscaleGuide } from "../components/tailscale-guide";

export function Tailscale() {
  const fetcher = useCallback(() => api.tailscaleStatus(), []);
  const [status, loading, refetch] = usePolling<TailscaleStatus>(fetcher, 10000);
  const [authKey, setAuthKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [savingPihole, setSavingPihole] = useState(false);
  const [piholeLines, setPiholeLines] = useState<string[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [toggling, setToggling] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [editingExitHostname, setEditingExitHostname] = useState(false);
  const [exitHostnameInput, setExitHostnameInput] = useState("");
  const piholeLogRef = useRef<HTMLDivElement>(null);

  const handleSave = async () => {
    setError("");
    const key = authKey.trim();
    if (key && !key.startsWith("tskey-auth-")) {
      setError("That doesn't look like a Tailscale auth key. It should start with \"tskey-auth-\".");
      return;
    }
    setSaving(true);
    try {
      await api.saveTailscaleConfig({
        enabled: true,
        auth_key: key || undefined,
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

  const handleToggle = async (appId: string, currentlyDisabled: boolean) => {
    setToggling(appId);
    try {
      await api.toggleAppTailscale(appId, !currentlyDisabled);
      refetch();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Toggle failed");
    } finally {
      setToggling(null);
    }
  };

  useEffect(() => {
    const el = piholeLogRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [piholeLines]);

  if (loading) {
    return (
      <div class="flex-1 flex items-center justify-center">
        <p class="text-gray-500">Loading...</p>
      </div>
    );
  }

  return (
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full space-y-4 sm:space-y-6">
      <h1 class="text-xl font-bold">Tailscale</h1>
      <p class="text-gray-400">
        Remote access to your apps via Tailscale. Each app gets its own
        HTTPS domain on your tailnet via a dedicated sidecar container.
      </p>

      {/* Status */}
      <section class="bg-gray-900 rounded-lg p-5 space-y-3">
        <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
          Status
        </h2>
        <div class="flex items-center gap-4 text-sm flex-wrap">
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
              Exit Node:{" "}
              <span
                class={
                  status?.exit_node_running ? "text-green-400" : "text-yellow-400"
                }
              >
                {status?.exit_node_running ? "Running" : "Stopped"}
              </span>
            </span>
          )}
          {status?.enabled && status?.pihole_installed && (
            <span class="text-gray-300">
              Pi-hole DNS:{" "}
              <span class={status?.pihole_dns ? "text-green-400" : "text-gray-500"}>
                {status?.pihole_dns ? "Active" : "Inactive"}
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
        {status?.enabled && status?.exit_node_running && (
          <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2 py-2 px-3 bg-gray-800 rounded">
            <div class="flex items-center gap-2 min-w-0">
              <span class="text-sm text-gray-300">Exit node hostname:</span>
              {editingExitHostname ? (
                <div class="flex items-center gap-2">
                  <input
                    type="text"
                    value={exitHostnameInput}
                    onInput={(e) => setExitHostnameInput((e.target as HTMLInputElement).value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        const name = exitHostnameInput.trim();
                        if (!name) return;
                        setSaving(true);
                        api.saveTailscaleConfig({
                          enabled: true,
                          exit_hostname: name,
                        }).then(() => {
                          setEditingExitHostname(false);
                          refetch();
                        }).catch((err: unknown) => {
                          setError(err instanceof Error ? err.message : "Failed to save");
                        }).finally(() => setSaving(false));
                      }
                      if (e.key === "Escape") setEditingExitHostname(false);
                    }}
                    class="px-2 py-1 bg-gray-900 border border-gray-600 rounded text-sm text-gray-100 font-mono w-48"
                    autoFocus
                  />
                  <button
                    onClick={() => {
                      const name = exitHostnameInput.trim();
                      if (!name) return;
                      setSaving(true);
                      api.saveTailscaleConfig({
                        enabled: true,
                        exit_hostname: name,
                      }).then(() => {
                        setEditingExitHostname(false);
                        refetch();
                      }).catch((err: unknown) => {
                        setError(err instanceof Error ? err.message : "Failed to save");
                      }).finally(() => setSaving(false));
                    }}
                    disabled={saving}
                    class="px-2 py-1 bg-amber-600 hover:bg-amber-500 text-white text-xs rounded disabled:opacity-50"
                  >
                    {saving ? "..." : "Save"}
                  </button>
                  <button
                    onClick={() => setEditingExitHostname(false)}
                    class="px-2 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
                  >
                    Cancel
                  </button>
                </div>
              ) : (
                <div class="flex items-center gap-2">
                  <span class="text-sm font-mono text-gray-100">
                    {status.exit_hostname || "myground"}
                  </span>
                  <button
                    onClick={() => {
                      setExitHostnameInput(status.exit_hostname || "myground");
                      setEditingExitHostname(true);
                    }}
                    class="px-2 py-1 text-xs bg-gray-700 hover:bg-gray-600 text-gray-300 rounded"
                  >
                    Rename
                  </button>
                </div>
              )}
            </div>
          </div>
        )}
      </section>

      {/* Exit node approval banner */}
      {status?.exit_node_running && status?.exit_node_approved === false && (
        <section class="bg-amber-900/20 border border-amber-500/30 rounded-lg p-4 flex gap-3">
          <span class="text-amber-400 shrink-0 text-lg">&#9888;</span>
          <div>
            <p class="text-sm font-medium text-amber-300">
              Exit node needs approval
            </p>
            <p class="text-xs text-gray-400 mt-1">
              Your exit node is running but hasn't been approved yet. Go to{" "}
              <a
                href="https://login.tailscale.com/admin/machines"
                target="_blank"
                rel="noopener noreferrer"
                class="text-amber-400 hover:text-amber-300 underline"
              >
                Tailscale Admin &gt; Machines
              </a>
              , find <span class="font-mono text-gray-300">myground</span>,
              click the <span class="font-medium text-gray-300">...</span> menu,
              and select{" "}
              <span class="font-medium text-gray-300">
                Edit route settings &gt; Use as exit node
              </span>
              .
            </p>
          </div>
        </section>
      )}

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
          <div class="space-y-3">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
              <p class="text-sm text-gray-300">
                Tailscale is enabled. Apps get individual sidecar containers for tailnet access.
              </p>
              <button
                onClick={handleDisable}
                disabled={saving}
                class="px-4 py-2 bg-red-600/80 hover:bg-red-500 text-white text-sm rounded disabled:opacity-50 shrink-0 self-start sm:self-auto"
              >
                {saving ? "Disabling..." : "Disable"}
              </button>
            </div>
            {status.exit_node_running && (
              <div class="space-y-2">
                <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-2 py-2 px-3 bg-gray-800 rounded">
                  <div>
                    <p class="text-sm text-gray-200">Route exit node DNS through Pi-hole</p>
                    <p class="text-xs text-gray-500">
                      {status.pihole_installed
                        ? "When enabled, all exit node traffic uses Pi-hole for ad blocking"
                        : "Install Pi-hole to enable network-wide ad blocking on your exit node"}
                    </p>
                  </div>
                  {status.pihole_installed ? (
                    <button
                      onClick={async () => {
                        setSavingPihole(true);
                        setError("");
                        setPiholeLines([]);
                        const success = await api.togglePiholeDns(
                          !status.pihole_dns,
                          (line) => setPiholeLines((prev) => [...prev, line]),
                        );
                        if (!success && piholeLines.length === 0) {
                          setError("Failed to toggle Pi-hole DNS");
                        }
                        setSavingPihole(false);
                        refetch();
                      }}
                      disabled={savingPihole || saving}
                      class={`px-3 py-1 text-xs rounded disabled:opacity-50 shrink-0 ${
                        savingPihole
                          ? "bg-amber-600 text-white"
                          : status.pihole_dns
                            ? "bg-red-600/80 hover:bg-red-500 text-white"
                            : "bg-green-600/80 hover:bg-green-500 text-white"
                      }`}
                    >
                      {savingPihole
                        ? status.pihole_dns ? "Disabling..." : "Enabling..."
                        : status.pihole_dns ? "Disable" : "Enable"}
                    </button>
                  ) : (
                    <a
                      href="/app/pihole"
                      class="px-3 py-1 text-xs rounded bg-amber-600 hover:bg-amber-500 text-white shrink-0"
                    >
                      Install Pi-hole
                    </a>
                  )}
                </div>
                {piholeLines.length > 0 && (
                  <div class="bg-gray-950 rounded border border-gray-800 overflow-hidden">
                    <div ref={piholeLogRef} class="max-h-40 overflow-y-auto p-3 font-mono text-xs leading-relaxed">
                      {piholeLines.map((line, i) => (
                        <div key={i} class={line.startsWith("Error") ? "text-red-400" : line.startsWith("Warning") ? "text-yellow-400" : "text-green-400"}>
                          {line}
                        </div>
                      ))}
                      {!savingPihole && !piholeLines.some(l => l.startsWith("Error")) && (
                        <button
                          onClick={() => setPiholeLines([])}
                          class="mt-2 text-gray-500 hover:text-gray-400 text-xs"
                        >
                          Clear
                        </button>
                      )}
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>
        )}

        {error && <p class="text-red-400 text-sm">{error}</p>}
      </section>

      {/* Per-app list */}
      {status?.enabled && status.apps.length > 0 && (
        <section class="bg-gray-900 rounded-lg p-5 space-y-4">
          <div class="flex items-center justify-between">
            <h2 class="text-sm font-medium text-gray-400 uppercase tracking-wider">
              Apps on Tailnet
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
            {status.apps.map((svc) => (
              <div
                key={svc.app_id}
                class="flex flex-col sm:flex-row sm:items-center justify-between gap-2 py-2 px-3 bg-gray-800 rounded"
              >
                <div class="flex items-center gap-2 min-w-0">
                  <span class="text-gray-200 font-medium shrink-0">
                    {svc.app_id}
                  </span>
                  {!svc.tailscale_disabled && (
                    <span
                      class={`text-xs px-1.5 py-0.5 rounded shrink-0 ${
                        svc.sidecar_running
                          ? "bg-green-900/50 text-green-400"
                          : "bg-yellow-900/50 text-yellow-400"
                      }`}
                    >
                      {svc.sidecar_running ? "running" : "stopped"}
                    </span>
                  )}
                </div>
                <div class="flex items-center gap-2 min-w-0">
                  {svc.url && !svc.tailscale_disabled ? (
                    <a
                      href={svc.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      class="text-amber-400 hover:text-amber-300 text-sm font-mono underline truncate"
                    >
                      {svc.url}
                    </a>
                  ) : svc.tailscale_disabled ? (
                    <span class="text-gray-500 text-sm">Disabled</span>
                  ) : (
                    <span class="text-gray-500 text-sm">
                      Not detected yet
                    </span>
                  )}
                  <button
                    onClick={() => handleToggle(svc.app_id, svc.tailscale_disabled)}
                    disabled={toggling === svc.app_id}
                    class={`px-3 py-1 text-xs rounded disabled:opacity-50 shrink-0 ${
                      svc.tailscale_disabled
                        ? "bg-green-600/80 hover:bg-green-500 text-white"
                        : "bg-gray-600 hover:bg-gray-500 text-gray-200"
                    }`}
                  >
                    {toggling === svc.app_id
                      ? "..."
                      : svc.tailscale_disabled
                        ? "Enable"
                        : "Disable"}
                  </button>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
