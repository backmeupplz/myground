import { useState, useEffect } from "preact/hooks";
import { api, type AppLink, type AvailableLinkOption } from "../api";

interface AppLinksProps {
  appId: string;
  appLinks: AppLink[];
  onUpdate: () => void;
}

export function AppLinks({ appId, appLinks, onUpdate }: AppLinksProps) {
  const [options, setOptions] = useState<AvailableLinkOption[] | null>(null);
  const [loadError, setLoadError] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    api
      .getAvailableLinks(appId)
      .then((res) => setOptions(res.links))
      .catch((e) =>
        setLoadError(e instanceof Error ? e.message : "Failed to load link options"),
      );
  }, [appId]);

  if (loadError) {
    return (
      <section class="bg-gray-900 rounded-lg p-4">
        <h3 class="text-sm font-medium text-gray-300">Connected Apps</h3>
        <p class="text-xs text-red-400 mt-1">{loadError}</p>
      </section>
    );
  }

  if (options === null) {
    return (
      <section class="bg-gray-900 rounded-lg p-4">
        <h3 class="text-sm font-medium text-gray-300">Connected Apps</h3>
        <p class="text-xs text-gray-500 mt-1">Loading...</p>
      </section>
    );
  }

  if (options.length === 0) {
    return null;
  }

  const handleChange = async (linkType: string, targetId: string | null) => {
    setSaving(true);
    setSaveError("");
    try {
      // Build updated links: keep all existing links except for this link_type,
      // then add the new one if a target was selected.
      const otherLinks = appLinks.filter((l) => l.link_type !== linkType) as AppLink[];
      const updatedLinks: AppLink[] = targetId
        ? [
            ...otherLinks,
            { target_id: targetId, link_type: linkType as AppLink["link_type"] },
          ]
        : otherLinks;
      await api.setAppLinks(appId, updatedLinks);
      onUpdate();
    } catch (e) {
      setSaveError(e instanceof Error ? e.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  return (
    <section class="bg-gray-900 rounded-lg p-4 space-y-3">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-sm font-medium text-gray-300">Connected Apps</h3>
          <p class="text-xs text-gray-500 mt-0.5">
            Link this app to other installed apps for automatic integration
          </p>
        </div>
        {saving && (
          <div class="flex items-center gap-1.5 text-xs text-gray-400">
            <svg
              class="animate-spin h-3.5 w-3.5 text-gray-400"
              viewBox="0 0 24 24"
              fill="none"
            >
              <circle
                class="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                stroke-width="4"
              />
              <path
                class="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
              />
            </svg>
            Saving & restarting...
          </div>
        )}
      </div>

      {saveError && <p class="text-red-400 text-xs">{saveError}</p>}

      {options.map((option) => {
        const currentLink = appLinks.find((l) => l.link_type === option.link_type);
        const currentTargetId = currentLink?.target_id ?? option.current_target_id ?? null;
        const isConnected = !!currentTargetId;
        const hasApps = option.available_apps.length > 0;
        const isSingle = option.available_apps.length === 1;
        const singleApp = isSingle ? option.available_apps[0] : null;
        const autoConnected =
          isSingle && singleApp && currentTargetId === singleApp.id;

        return (
          <div
            key={option.link_type}
            class="flex items-center justify-between py-2 border-t border-gray-800"
          >
            <div class="flex items-center gap-2">
              <div
                class={`w-2 h-2 rounded-full ${isConnected ? "bg-green-500" : "bg-gray-600"}`}
              />
              <span class="text-sm text-gray-300">{option.label}</span>
            </div>
            <div class="flex items-center gap-2">
              {!hasApps ? (
                <span class="text-xs text-gray-600">No compatible apps installed</span>
              ) : autoConnected ? (
                <div class="flex items-center gap-2">
                  <span class="text-xs text-green-400">
                    Auto-connected to {singleApp!.display_name}
                  </span>
                  <button
                    class="text-xs text-gray-500 hover:text-gray-300 disabled:opacity-50"
                    disabled={saving}
                    onClick={() => handleChange(option.link_type, null)}
                  >
                    Disconnect
                  </button>
                </div>
              ) : isSingle && singleApp ? (
                <div class="flex items-center gap-2">
                  <select
                    class="px-2 py-1 bg-gray-800 border border-gray-700 rounded text-gray-100 text-xs focus:outline-none focus:border-gray-500"
                    value={currentTargetId ?? ""}
                    disabled={saving}
                    onChange={(e) => {
                      const val = (e.target as HTMLSelectElement).value;
                      handleChange(option.link_type, val || null);
                    }}
                  >
                    <option value="">Not connected</option>
                    <option value={singleApp.id}>{singleApp.display_name}</option>
                  </select>
                </div>
              ) : (
                <select
                  class="px-2 py-1 bg-gray-800 border border-gray-700 rounded text-gray-100 text-xs focus:outline-none focus:border-gray-500"
                  value={currentTargetId ?? ""}
                  disabled={saving}
                  onChange={(e) => {
                    const val = (e.target as HTMLSelectElement).value;
                    handleChange(option.link_type, val || null);
                  }}
                >
                  <option value="">Not connected</option>
                  {option.available_apps.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.display_name}
                    </option>
                  ))}
                </select>
              )}
            </div>
          </div>
        );
      })}
    </section>
  );
}
