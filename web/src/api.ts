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
  backup_supported: boolean;
  containers: ContainerStatus[];
  storage: StorageVolumeStatus[];
  port: number | null;
  install_variables: InstallVariable[];
  env_overrides: Record<string, string>;
  backup_password: string | null;
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

export interface InstallVariable {
  key: string;
  label: string;
  input_type: string;
  required: boolean;
  default?: string;
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

export interface AvailableService {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  multi_instance: boolean;
  backup_supported: boolean;
  website: string;
  install_variables: InstallVariable[];
  has_storage?: boolean;
}

export interface SystemStats {
  cpu_usage_percent: number;
  cpu_count: number;
  cpu_brand: string;
  ram_total_bytes: number;
  ram_used_bytes: number;
}

export interface DirEntry {
  name: string;
  path: string;
}

export interface BrowseResult {
  path: string;
  entries: DirEntry[];
}

export interface GlobalConfig {
  version: string;
  default_storage_path?: string;
  backup?: BackupConfig;
}

// ── Utilities ─────────────────────────────────────────────────────────────

export function generatePassword(length: number): string {
  const chars =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.~";
  const arr = new Uint8Array(length);
  crypto.getRandomValues(arr);
  return Array.from(arr, (b) => chars[b % chars.length]).join("");
}

export function containerColor(c: ContainerStatus): string {
  if (c.state === "running") return "text-green-400";
  if (c.state === "created") return "text-gray-400";
  if (isSuccessfulExit(c)) return "text-gray-400";
  return "text-red-400";
}

export function containerIcon(c: ContainerStatus): string {
  if (c.state === "running") return "\u2713";
  if (isSuccessfulExit(c)) return "\u2713";
  return "\u25cb";
}

function isSuccessfulExit(c: ContainerStatus): boolean {
  return c.state === "exited" && c.status.includes("(0)");
}

export function isReady(containers: ContainerStatus[]): boolean {
  if (containers.length === 0) return false;
  return containers.every((c) => c.state === "running" || isSuccessfulExit(c));
}

export function isCrashLooping(containers: ContainerStatus[]): boolean {
  return containers.some(
    (c) =>
      c.status.includes("Restarting") ||
      (c.state === "exited" && !isSuccessfulExit(c)) ||
      c.state === "dead",
  );
}

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

  stats: () => request<SystemStats>("/api/stats"),

  browse: (path = "/") =>
    request<BrowseResult>(`/api/browse?path=${encodeURIComponent(path)}`),

  services: () => request<ServiceInfo[]>("/api/services"),

  availableServices: () =>
    request<AvailableService[]>("/api/services/available"),

  installService: (
    id: string,
    body?: {
      storage_path?: string;
      variables?: Record<string, string>;
      display_name?: string;
    },
  ) =>
    request<InstallResponse>(`/api/services/${id}/install`, {
      method: "POST",
      ...jsonBody(body ?? {}),
    }),

  renameService: (id: string, displayName: string) =>
    request<ActionResponse>(`/api/services/${id}/rename`, {
      method: "PUT",
      ...jsonBody({ display_name: displayName }),
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

  dismissCredentials: (id: string) =>
    request<ActionResponse>(`/api/services/${id}/dismiss-credentials`, {
      method: "POST",
    }),

  dismissBackupPassword: (id: string) =>
    request<ActionResponse>(`/api/services/${id}/dismiss-backup-password`, {
      method: "POST",
    }),

  backupConfig: () => request<BackupConfig>("/api/backup/config"),

  globalConfig: () => request<GlobalConfig>("/api/config"),

  saveGlobalConfig: (config: GlobalConfig) =>
    request<ActionResponse>("/api/config", {
      method: "PUT",
      ...jsonBody(config),
    }),
};
