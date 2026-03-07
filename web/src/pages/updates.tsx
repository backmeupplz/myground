import { useState, useEffect } from "preact/hooks";
import {
  api,
  formatTimestamp,
  shortDigest,
  type UpdateStatus,
  type UpdateConfig,
} from "../api";

export function Updates() {
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [updateConfig, setUpdateConfig] = useState<UpdateConfig | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkStatus, setCheckStatus] = useState("");
  const [selfUpdating, setSelfUpdating] = useState(false);
  const [selfUpdateLines, setSelfUpdateLines] = useState<string[]>([]);
  const [selfUpdateDone, setSelfUpdateDone] = useState(false);
  const [updatingApps, setUpdatingApps] = useState<Record<string, boolean>>({});
  const [updateLines, setUpdateLines] = useState<Record<string, string[]>>({});

  useEffect(() => {
    api.updateStatus().then(setUpdateStatus).catch(() => {});
    api.updateConfig().then(setUpdateConfig).catch(() => {});
  }, []);

  const handleAppUpdate = (appId: string) => {
    setUpdatingApps((prev) => ({ ...prev, [appId]: true }));
    setUpdateLines((prev) => ({ ...prev, [appId]: [] }));
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(
      `${proto}//${window.location.host}/api/apps/${appId}/update`,
    );
    ws.onmessage = (e) => {
      const msg = e.data;
      if (msg === "__DONE__") {
        ws.close();
        setUpdatingApps((prev) => ({ ...prev, [appId]: false }));
        api.updateStatus().then(setUpdateStatus).catch(() => {});
      } else {
        setUpdateLines((prev) => ({
          ...prev,
          [appId]: [...(prev[appId] || []), msg],
        }));
      }
    };
    ws.onerror = () => {
      setUpdatingApps((prev) => ({ ...prev, [appId]: false }));
    };
    ws.onclose = () => {
      setUpdatingApps((prev) => ({ ...prev, [appId]: false }));
    };
  };

  const appsWithUpdates =
    updateStatus?.apps.filter((a) => a.update_available) ?? [];

  return (
    <div class="flex-1 px-3 sm:px-6 py-4 sm:py-6 max-w-4xl mx-auto w-full">
      <h1 class="text-xl font-bold mb-6">Updates</h1>

      {/* MyGround */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          MyGround
        </h2>
        <div class="flex items-center justify-between">
          <div>
            <span class="text-sm text-gray-200">MyGround</span>
            <span class="text-xs text-gray-500 ml-2">
              v{updateStatus?.myground_version ?? "..."}
            </span>
            {updateStatus?.myground_update_available && (
              <span class="ml-2 text-xs text-blue-400">
                v{updateStatus.latest_myground_version} available
              </span>
            )}
          </div>
          {updateStatus?.myground_update_available && (
            <button
              class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
              disabled={selfUpdating}
              onClick={() => {
                setSelfUpdating(true);
                setSelfUpdateLines([]);
                setSelfUpdateDone(false);
                const proto =
                  window.location.protocol === "https:" ? "wss:" : "ws:";
                const ws = new WebSocket(
                  `${proto}//${window.location.host}/api/updates/self-update`,
                );
                ws.onmessage = (e) => {
                  const msg = e.data;
                  if (msg === "__DONE__") {
                    ws.close();
                    setSelfUpdateDone(true);
                    setSelfUpdateLines((prev) => [
                      ...prev,
                      "Update complete, restarting...",
                    ]);
                    // Poll until new version is live, then reload to get new assets
                    const poll = setInterval(async () => {
                      try {
                        await api.updateStatus();
                        clearInterval(poll);
                        window.location.reload();
                      } catch {
                        // server still restarting
                      }
                    }, 2000);
                  } else if (msg.startsWith("Error:")) {
                    setSelfUpdateLines((prev) => [...prev, msg]);
                    ws.close();
                    setSelfUpdating(false);
                  } else {
                    setSelfUpdateLines((prev) => [...prev, msg]);
                  }
                };
                ws.onerror = () => {
                  // Connection lost — likely the server restarted after update
                  setSelfUpdateLines((prev) => [
                    ...prev,
                    "Connection lost, waiting for restart...",
                  ]);
                  const poll = setInterval(async () => {
                    try {
                      await api.updateStatus();
                      clearInterval(poll);
                      window.location.reload();
                    } catch {
                      // server still restarting
                    }
                  }, 2000);
                };
                ws.onclose = () => {
                  if (!selfUpdateDone) {
                    // If lines suggest the update was in progress, poll for restart
                    setSelfUpdateLines((prev) => {
                      const hasRestart = prev.some(
                        (l) =>
                          l.includes("Restarting") ||
                          l.includes("Installing update"),
                      );
                      if (hasRestart) {
                        const poll = setInterval(async () => {
                          try {
                            await api.updateStatus();
                            clearInterval(poll);
                            window.location.reload();
                          } catch {
                            // server still restarting
                          }
                        }, 2000);
                        return [...prev, "Waiting for restart..."];
                      }
                      setSelfUpdating(false);
                      return prev;
                    });
                  }
                };
              }}
            >
              {selfUpdating ? "Updating..." : "Upgrade"}
            </button>
          )}
        </div>
        {selfUpdateLines.length > 0 && (
          <pre
            class="mt-2 bg-gray-950 rounded p-3 text-xs text-gray-300 max-h-48 overflow-y-auto font-mono"
            ref={(el) => {
              if (el) el.scrollTop = el.scrollHeight;
            }}
          >
            {selfUpdateLines.join("\n")}
          </pre>
        )}
      </section>

      {/* Apps */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Apps
        </h2>
        {appsWithUpdates.length === 0 ? (
          <p class="text-sm text-gray-500">All apps are up to date</p>
        ) : (
          <div class="space-y-3">
            {appsWithUpdates.map((app) => (
              <div key={app.id}>
                <div class="flex items-center justify-between bg-gray-900 rounded-lg px-4 py-3">
                  <div>
                    <span class="text-sm text-gray-200">{app.name}</span>
                    {app.current_digest && app.latest_digest && (
                      <span class="inline-flex items-center text-xs ml-2 gap-1">
                        <span class="font-mono text-gray-500">{shortDigest(app.current_digest)}</span>
                        <svg class="w-3 h-3 text-gray-500" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24"><path d="M13.5 4.5 21 12m0 0-7.5 7.5M21 12H3" /></svg>
                        <span class="font-mono text-blue-400">{shortDigest(app.latest_digest)}</span>
                      </span>
                    )}
                  </div>
                  <button
                    class="px-3 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded disabled:opacity-50"
                    disabled={updatingApps[app.id]}
                    onClick={() => handleAppUpdate(app.id)}
                  >
                    {updatingApps[app.id] ? "Updating..." : "Update"}
                  </button>
                </div>
                {(updateLines[app.id]?.length ?? 0) > 0 && (
                  <pre class="mt-1 bg-gray-950 rounded p-3 text-xs text-gray-300 max-h-48 overflow-y-auto font-mono">
                    {updateLines[app.id].join("\n")}
                  </pre>
                )}
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Config */}
      <section class="mb-8">
        <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Configuration
        </h2>
        {updateConfig && (
          <div class="space-y-3 mb-4">
            <label class="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={updateConfig.auto_update_apps}
                onChange={async (e) => {
                  const val = (e.target as HTMLInputElement).checked;
                  const newCfg = { ...updateConfig, auto_update_apps: val };
                  setUpdateConfig(newCfg);
                  await api.saveUpdateConfig({
                    auto_update_apps: val,
                    auto_update_myground: newCfg.auto_update_myground,
                  });
                }}
                class="rounded"
              />
              <span class="text-sm text-gray-300">Auto-update apps</span>
            </label>
            <label class="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={updateConfig.auto_update_myground}
                onChange={async (e) => {
                  const val = (e.target as HTMLInputElement).checked;
                  const newCfg = {
                    ...updateConfig,
                    auto_update_myground: val,
                  };
                  setUpdateConfig(newCfg);
                  await api.saveUpdateConfig({
                    auto_update_apps: newCfg.auto_update_apps,
                    auto_update_myground: val,
                  });
                }}
                class="rounded"
              />
              <span class="text-sm text-gray-300">Auto-update MyGround</span>
            </label>
          </div>
        )}

        <div class="flex items-center gap-3">
          <button
            class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50"
            disabled={checking}
            onClick={async () => {
              setChecking(true);

              // Snapshot per-app last_check timestamps to detect progress
              const initialAppChecks = new Map<string, string | null>();
              const appNames = new Map<string, string>();
              const appIds: string[] = [];
              const initialGlobalCheck = updateStatus?.last_check ?? null;
              for (const app of updateStatus?.apps ?? []) {
                initialAppChecks.set(app.id, app.last_check);
                appNames.set(app.id, app.name);
                appIds.push(app.id);
              }
              const totalApps = appIds.length;

              if (totalApps > 0) {
                setCheckStatus(
                  `Pulling ${appNames.get(appIds[0])} image...`,
                );
              } else {
                setCheckStatus("Checking for updates...");
              }

              try {
                await api.updateCheck();
              } catch {
                setChecking(false);
                setCheckStatus("");
                return;
              }

              // Poll for progress
              for (let i = 0; i < 30; i++) {
                await new Promise((r) => setTimeout(r, 2000));
                try {
                  const status = await api.updateStatus();
                  setUpdateStatus(status);

                  // Find which apps have been checked
                  const checkedIds = new Set<string>();
                  for (const app of status.apps) {
                    const initial = initialAppChecks.get(app.id);
                    if (app.last_check && app.last_check !== initial) {
                      checkedIds.add(app.id);
                    }
                  }

                  const mgDone =
                    status.last_check !== null &&
                    status.last_check !== initialGlobalCheck;

                  if (checkedIds.size < totalApps) {
                    // Find the next unchecked app
                    const nextId = appIds.find((id) => !checkedIds.has(id));
                    const nextName = nextId
                      ? appNames.get(nextId) ?? nextId
                      : "...";
                    setCheckStatus(
                      `Pulling ${nextName} image (${checkedIds.size + 1}/${totalApps})...`,
                    );
                  } else if (!mgDone) {
                    setCheckStatus("Checking MyGround version...");
                  } else {
                    // All done
                    const updates = status.apps.filter(
                      (a) => a.update_available,
                    ).length;
                    setCheckStatus(
                      updates > 0
                        ? `Done — ${updates} update(s) available`
                        : "Done — everything is up to date",
                    );
                    setChecking(false);
                    setTimeout(() => setCheckStatus(""), 3000);
                    return;
                  }
                } catch {
                  // continue polling
                }
              }

              // Timeout — fetch final status
              const final_ = await api.updateStatus().catch(() => null);
              if (final_) setUpdateStatus(final_);
              setChecking(false);
              setCheckStatus("");
            }}
          >
            {checking ? "Checking..." : "Check Now"}
          </button>
          {checkStatus ? (
            <span class="text-xs text-gray-400">{checkStatus}</span>
          ) : (
            updateStatus?.last_check && (
              <span class="text-xs text-gray-500">
                Last checked: {formatTimestamp(updateStatus.last_check)}
              </span>
            )
          )}
        </div>
      </section>
    </div>
  );
}
