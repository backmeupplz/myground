import { useState, useEffect } from "preact/hooks";

export interface DiskInfo {
  name: string;
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  fs_type: string;
  is_removable: boolean;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 4) return (bytes / 1024 ** 4).toFixed(1) + " TB";
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(1) + " MB";
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
  return bytes + " B";
}

export function DiskList() {
  const [disks, setDisks] = useState<DiskInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch("/api/disks")
      .then((r) => r.json())
      .then((data) => {
        setDisks(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  if (loading) {
    return <p class="text-gray-500">Loading disks...</p>;
  }

  if (disks.length === 0) {
    return <p class="text-gray-500">No disks found.</p>;
  }

  return (
    <div class="w-full max-w-2xl space-y-3">
      {disks.map((disk) => {
        const pct =
          disk.total_bytes > 0
            ? Math.round((disk.used_bytes / disk.total_bytes) * 100)
            : 0;
        const barColor =
          pct > 90
            ? "bg-red-500"
            : pct > 70
              ? "bg-yellow-500"
              : "bg-green-500";

        return (
          <div
            key={disk.mount_point}
            class="bg-gray-900 rounded-lg p-4"
          >
            <div class="flex justify-between items-center mb-2">
              <div>
                <h3 class="font-semibold text-gray-100">
                  {disk.mount_point}
                </h3>
                <p class="text-xs text-gray-500">
                  {disk.fs_type}
                  {disk.is_removable ? " · Removable" : ""}
                </p>
              </div>
              <span class="text-sm text-gray-400">
                {formatBytes(disk.available_bytes)} free of{" "}
                {formatBytes(disk.total_bytes)}
              </span>
            </div>
            <div class="w-full bg-gray-700 rounded-full h-2">
              <div
                class={`${barColor} h-2 rounded-full`}
                style={{ width: `${pct}%` }}
              />
            </div>
            <p class="text-xs text-gray-500 mt-1 text-right">{pct}% used</p>
          </div>
        );
      })}
    </div>
  );
}
