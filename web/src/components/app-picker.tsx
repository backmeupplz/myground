import { useState, useEffect, useRef } from "preact/hooks";
import { api, type AvailableApp } from "../api";
import { AppIcon } from "./app-icon";

interface Props {
  onSelect: (app: AvailableApp) => void;
  onClose: () => void;
}

export function AppPicker({ onSelect, onClose }: Props) {
  const [apps, setApps] = useState<AvailableApp[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api
      .availableApps()
      .then((data) => {
        setApps(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  useEffect(() => {
    inputRef.current?.focus();
  }, [loading]);

  const lowerQuery = query.toLowerCase();
  const filtered = apps.filter(
    (s) =>
      s.name.toLowerCase().includes(lowerQuery) ||
      s.description.toLowerCase().includes(lowerQuery) ||
      s.category.toLowerCase().includes(lowerQuery),
  );

  return (
    <div
      class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4"
      onClick={onClose}
    >
      <div
        class="bg-gray-900 rounded-xl max-w-lg w-full p-6 max-h-[80vh] flex flex-col"
        onClick={(e: Event) => e.stopPropagation()}
      >
        <div class="flex items-center justify-between mb-4">
          <h2 class="text-lg font-bold text-gray-100">Add App</h2>
          <button
            class="text-gray-500 hover:text-gray-300 text-xl"
            onClick={onClose}
          >
            &times;
          </button>
        </div>

        <input
          ref={inputRef}
          type="text"
          placeholder="Search apps..."
          value={query}
          onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
          class="w-full bg-gray-800 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-200 mb-4 focus:outline-none focus:border-gray-500"
        />

        <div class="overflow-y-auto flex-1 space-y-2">
          {loading && (
            <p class="text-gray-500 text-sm text-center py-4">Loading...</p>
          )}
          {!loading && filtered.length === 0 && (
            <p class="text-gray-500 text-sm text-center py-4">
              No apps found.
            </p>
          )}
          {filtered.map((svc) => (
            <button
              key={svc.id}
              class="w-full text-left bg-gray-800 hover:bg-gray-700 rounded-lg p-4 transition-colors"
              onClick={() => onSelect(svc)}
            >
              <div class="flex items-start gap-3">
                <AppIcon id={svc.id} class="w-5 h-5 shrink-0 mt-0.5" />
                <div class="min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <span class="font-semibold text-gray-100">{svc.name}</span>
                    <span class="text-xs text-gray-500">{svc.category}</span>
                  </div>
                  <p class="text-sm text-gray-400 line-clamp-2">
                    {svc.description}
                  </p>
                </div>
              </div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
