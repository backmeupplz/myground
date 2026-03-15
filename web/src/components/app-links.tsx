import { useState, useEffect } from "preact/hooks";
import { api, type AppLink, type AvailableLinkGroup } from "../api";

interface AppLinksProps {
  appId: string;
  appLinks: AppLink[];
  onUpdate: () => void;
}

export function AppLinks({ appId, appLinks, onUpdate }: AppLinksProps) {
  const [groups, setGroups] = useState<AvailableLinkGroup[] | null>(null);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api
      .getAvailableLinks(appId)
      .then(setGroups)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load"));
  }, [appId]);

  if (error) {
    return (
      <section class="bg-gray-900 rounded-lg p-4">
        <h3 class="text-sm font-medium text-gray-300">Connected Apps</h3>
        <p class="text-xs text-red-400 mt-1">{error}</p>
      </section>
    );
  }

  if (!groups || groups.length === 0) return null;

  const handleChange = async (linkType: string, targetId: string | null) => {
    setSaving(true);
    setError("");
    try {
      const otherLinks = appLinks.filter((l) => l.link_type !== linkType);
      const updatedLinks: AppLink[] = targetId
        ? [...otherLinks, { target_id: targetId, link_type: linkType as AppLink["link_type"] }]
        : otherLinks;
      await api.setAppLinks(appId, updatedLinks);
      onUpdate();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save");
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
          <span class="text-xs text-gray-400 animate-pulse">Saving & restarting...</span>
        )}
      </div>

      {error && <p class="text-red-400 text-xs">{error}</p>}

      {groups.map((group) => {
        const currentLink = appLinks.find((l) => l.link_type === group.link_type);
        const linkedTarget = group.available_targets.find((t) => t.currently_linked);
        const currentTargetId = currentLink?.target_id ?? linkedTarget?.instance_id ?? null;
        const isConnected = !!currentTargetId;

        return (
          <div key={group.link_type} class="flex items-center justify-between py-2 border-t border-gray-800">
            <div class="flex items-center gap-2">
              <div class={`w-2 h-2 rounded-full ${isConnected ? "bg-green-500" : "bg-gray-600"}`} />
              <span class="text-sm text-gray-300">{group.label}</span>
            </div>
            <div>
              {group.available_targets.length === 0 ? (
                <span class="text-xs text-gray-600">No compatible apps installed</span>
              ) : (
                <select
                  class="px-2 py-1 bg-gray-800 border border-gray-700 rounded text-gray-100 text-xs focus:outline-none focus:border-gray-500"
                  value={currentTargetId ?? ""}
                  disabled={saving}
                  onChange={(e) => {
                    const val = (e.target as HTMLSelectElement).value;
                    handleChange(group.link_type, val || null);
                  }}
                >
                  <option value="">Not connected</option>
                  {group.available_targets.map((t) => (
                    <option key={t.instance_id} value={t.instance_id}>
                      {t.display_name}
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
