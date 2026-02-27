// ── Types ──────────────────────────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  version: string;
}

export interface ContainerStatus {
  name: string;
  state: string;
  status: string;
}

export interface StorageVolumeStatus {
  name: string;
  container_path: string;
  host_path: string;
  disk_available_bytes: number | null;
}

export interface ServiceInfo {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  installed: boolean;
  has_storage: boolean;
  containers: ContainerStatus[];
  storage: StorageVolumeStatus[];
  port: number | null;
}

export interface DiskInfo {
  name: string;
  mount_point: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  fs_type: string;
  is_removable: boolean;
}

export interface BackupConfig {
  repository?: string;
  password?: string;
  keep_daily?: number;
  keep_weekly?: number;
  keep_monthly?: number;
  s3_access_key?: string;
  s3_secret_key?: string;
}

export interface ServiceBackupConfig {
  enabled: boolean;
  local?: BackupConfig;
  remote?: BackupConfig;
}

export interface InstallResponse {
  ok: boolean;
  message: string;
  port: number;
}

export interface ActionResponse {
  ok: boolean;
  message: string;
}

// ── Utilities ─────────────────────────────────────────────────────────────

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 4) return (bytes / 1024 ** 4).toFixed(1) + " TB";
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(1) + " MB";
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
  return bytes + " B";
}

// ── Fetch wrapper ──────────────────────────────────────────────────────────

async function request<T>(url: string, options?: RequestInit): Promise<T> {
  const res = await fetch(url, options);
  if (!res.ok) {
    const body = await res.json().catch(() => ({ message: res.statusText }));
    throw new Error(body.message || res.statusText);
  }
  return res.json();
}

function jsonBody(data: unknown): RequestInit {
  return {
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  };
}

// ── API methods ────────────────────────────────────────────────────────────

export const api = {
  health: () => request<HealthResponse>("/api/health"),

  services: () => request<ServiceInfo[]>("/api/services"),

  installService: (id: string, body?: { storage_path?: string }) =>
    request<InstallResponse>(`/api/services/${id}/install`, {
      method: "POST",
      ...jsonBody(body ?? {}),
    }),

  startService: (id: string) =>
    request<ActionResponse>(`/api/services/${id}/start`, { method: "POST" }),

  stopService: (id: string) =>
    request<ActionResponse>(`/api/services/${id}/stop`, { method: "POST" }),

  removeService: (id: string) =>
    request<ActionResponse>(`/api/services/${id}`, { method: "DELETE" }),

  disks: () => request<DiskInfo[]>("/api/disks"),

  getServiceBackup: (id: string) =>
    request<ServiceBackupConfig>(`/api/services/${id}/backup`),

  updateServiceBackup: (id: string, config: ServiceBackupConfig) =>
    request<ActionResponse>(`/api/services/${id}/backup`, {
      method: "PUT",
      ...jsonBody(config),
    }),

  updateStorage: (id: string, paths: Record<string, string>) =>
    request<ActionResponse>(`/api/services/${id}/storage`, {
      method: "PUT",
      ...jsonBody({ paths }),
    }),

  backupConfig: () => request<BackupConfig>("/api/backup/config"),
};
