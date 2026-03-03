// ── Types ──────────────────────────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  version: string;
  server_ip?: string;
}

export interface AuthStatus {
  setup_required: boolean;
  authenticated: boolean;
}

export interface LoginResponse {
  ok: boolean;
  message: string;
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

export interface AppInfo {
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
  has_backup_password: boolean;
  post_install_notes?: string | null;
  web_path?: string | null;
  tailscale_url?: string | null;
  tailscale_disabled: boolean;
  tailscale_hostname?: string | null;
  lan_accessible: boolean;
  uses_host_network: boolean;
  update_available: boolean;
  domain_url?: string | null;
  supports_gpu: boolean;
  gpu_mode: string | null;
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
  s3_access_key?: string;
  s3_secret_key?: string;
}

export interface AppBackupConfig {
  enabled: boolean;
  local?: BackupConfig;
  remote?: BackupConfig;
  schedule?: string;
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

export interface AvailableApp {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  backup_supported: boolean;
  website: string;
  install_variables: InstallVariable[];
  has_storage?: boolean;
  post_install_notes?: string | null;
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

export interface Snapshot {
  id: string;
  time: string;
  paths: string[];
  tags: string[];
  hostname: string;
}

export interface BackupResult {
  snapshot_id: string;
  files_new: number;
  bytes_added: number;
}

export interface TailscaleStatus {
  enabled: boolean;
  exit_node_running: boolean;
  exit_node_approved: boolean | null;
  tailnet: string | null;
  apps: TailscaleAppInfo[];
}

export interface TailscaleAppInfo {
  app_id: string;
  hostname: string;
  url: string | null;
  sidecar_running: boolean;
  tailscale_disabled: boolean;
}

export interface CloudflareStatus {
  enabled: boolean;
  tunnel_running: boolean;
  tunnel_id: string | null;
  bindings: CloudflareBinding[];
}

export interface CloudflareBinding {
  app_id: string;
  app_name: string;
  fqdn: string;
  subdomain: string;
  zone_name: string;
}

export interface CloudflareZone {
  id: string;
  name: string;
}

export interface DomainBinding {
  subdomain: string;
  zone_id: string;
  zone_name: string;
  dns_record_id?: string;
}

export interface ApiKeyInfo {
  id: string;
  name: string;
  created_at: string;
}

export interface CreateApiKeyResponse {
  ok: boolean;
  id: string;
  name: string;
  key: string;
}

export interface AppUpdateInfo {
  id: string;
  update_available: boolean;
  last_check: string | null;
}

export interface UpdateStatus {
  myground_version: string;
  latest_myground_version: string | null;
  myground_update_available: boolean;
  apps: AppUpdateInfo[];
  last_check: string | null;
}

export interface UpdateConfig {
  auto_update_apps: boolean;
  auto_update_myground: boolean;
  last_check: string | null;
  latest_myground_version: string | null;
  latest_myground_url: string | null;
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

export function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 4) return (bytes / 1024 ** 4).toFixed(1) + " TB";
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(1) + " GB";
  if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(1) + " MB";
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
  return bytes + " B";
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function linkify(text: string): string {
  const escaped = escapeHtml(text);
  return escaped.replace(
    /(https?:\/\/[^\s<]+)/g,
    '<a href="$1" target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline">$1</a>',
  );
}

// ── Fetch wrapper ──────────────────────────────────────────────────────────

/** Callback set by the app to handle 401 responses (e.g. redirect to login). */
let onUnauthorized: (() => void) | null = null;

export function setOnUnauthorized(cb: () => void) {
  onUnauthorized = cb;
}

async function request<T>(url: string, options?: RequestInit): Promise<T> {
  const res = await fetch(url, options);
  if (res.status === 401 && onUnauthorized) {
    onUnauthorized();
    throw new Error("Not authenticated");
  }
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
  // Auth
  authStatus: () => request<AuthStatus>("/api/auth/status"),

  setup: (body: {
    username: string;
    password: string;
    tailscale_key?: string;
  }) =>
    request<LoginResponse>("/api/auth/setup", {
      method: "POST",
      ...jsonBody(body),
    }),

  login: (username: string, password: string) =>
    request<LoginResponse>("/api/auth/login", {
      method: "POST",
      ...jsonBody({ username, password }),
    }),

  logout: () =>
    request<LoginResponse>("/api/auth/logout", { method: "POST" }),

  // Health
  health: () => request<HealthResponse>("/api/health"),

  stats: () => request<SystemStats>("/api/stats"),

  browse: (path = "/") =>
    request<BrowseResult>(`/api/browse?path=${encodeURIComponent(path)}`),

  mkdir: (path: string) =>
    request<BrowseResult>("/api/mkdir", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path }),
    }),

  apps: () => request<AppInfo[]>("/api/apps"),

  availableApps: () =>
    request<AvailableApp[]>("/api/apps/available"),

  installApp: (
    id: string,
    body?: {
      storage_path?: string;
      variables?: Record<string, string>;
      display_name?: string;
    },
  ) =>
    request<InstallResponse>(`/api/apps/${id}/install`, {
      method: "POST",
      ...jsonBody(body ?? {}),
    }),

  renameApp: (id: string, displayName: string) =>
    request<ActionResponse>(`/api/apps/${id}/rename`, {
      method: "PUT",
      ...jsonBody({ display_name: displayName }),
    }),

  startApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/start`, { method: "POST" }),

  stopApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/stop`, { method: "POST" }),

  removeApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}`, { method: "DELETE" }),

  disks: () => request<DiskInfo[]>("/api/disks"),

  getAppBackup: (id: string) =>
    request<AppBackupConfig>(`/api/apps/${id}/backup`),

  updateAppBackup: (id: string, config: AppBackupConfig) =>
    request<ActionResponse>(`/api/apps/${id}/backup`, {
      method: "PUT",
      ...jsonBody(config),
    }),

  updateStorage: (id: string, paths: Record<string, string>) =>
    request<ActionResponse>(`/api/apps/${id}/storage`, {
      method: "PUT",
      ...jsonBody({ paths }),
    }),

  dismissCredentials: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/dismiss-credentials`, {
      method: "POST",
    }),

  dismissBackupPassword: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/dismiss-backup-password`, {
      method: "POST",
    }),

  getBackupPassword: (id: string) =>
    request<{ password: string | null }>(`/api/apps/${id}/backup-password`),

  backupRunAll: () =>
    request<BackupResult[]>("/api/backup/run", { method: "POST" }),

  backupSnapshots: () => request<Snapshot[]>("/api/backup/snapshots"),

  backupRestore: (snapshotId: string, targetPath: string) =>
    request<ActionResponse>(`/api/backup/restore/${snapshotId}`, {
      method: "POST",
      ...jsonBody({ target_path: targetPath }),
    }),

  appBackupSnapshots: (id: string) =>
    request<Snapshot[]>(`/api/apps/${id}/backup/snapshots`),

  appBackupRun: (id: string) =>
    request<BackupResult[]>(`/api/apps/${id}/backup/run`, {
      method: "POST",
    }),

  backupConfig: () => request<BackupConfig>("/api/backup/config"),

  globalConfig: () => request<GlobalConfig>("/api/config"),

  saveGlobalConfig: (config: GlobalConfig) =>
    request<ActionResponse>("/api/config", {
      method: "PUT",
      ...jsonBody(config),
    }),

  // Tailscale
  tailscaleStatus: () => request<TailscaleStatus>("/api/tailscale/status"),

  saveTailscaleConfig: (body: {
    enabled: boolean;
    auth_key?: string | null;
  }) =>
    request<ActionResponse>("/api/tailscale/config", {
      method: "PUT",
      ...jsonBody(body),
    }),

  tailscaleRefresh: () =>
    request<ActionResponse>("/api/tailscale/refresh", { method: "POST" }),

  toggleAppTailscale: (id: string, disabled: boolean, hostname?: string) =>
    request<ActionResponse>(`/api/apps/${id}/tailscale`, {
      method: "PUT",
      ...jsonBody({ disabled, hostname }),
    }),

  toggleAppLan: (id: string, enabled: boolean) =>
    request<ActionResponse>(`/api/apps/${id}/lan`, {
      method: "PUT",
      ...jsonBody({ enabled }),
    }),

  setAppGpu: (id: string, mode: string) =>
    request<ActionResponse>(`/api/apps/${id}/gpu`, {
      method: "PUT",
      ...jsonBody({ mode }),
    }),

  // Cloudflare
  cloudflareStatus: () => request<CloudflareStatus>("/api/cloudflare/status"),

  saveCloudflareConfig: (body: { enabled: boolean; api_token?: string }) =>
    request<ActionResponse>("/api/cloudflare/config", {
      method: "PUT",
      ...jsonBody(body),
    }),

  cloudflareZones: () => request<CloudflareZone[]>("/api/cloudflare/zones"),

  bindDomain: (
    id: string,
    body: { subdomain: string; zone_id: string; zone_name: string },
  ) =>
    request<DomainBinding>(`/api/apps/${id}/domain`, {
      method: "PUT",
      ...jsonBody(body),
    }),

  unbindDomain: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/domain`, {
      method: "DELETE",
    }),

  // API Keys
  listApiKeys: () => request<ApiKeyInfo[]>("/api/auth/api-keys"),

  createApiKey: (name: string) =>
    request<CreateApiKeyResponse>("/api/auth/api-keys", {
      method: "POST",
      ...jsonBody({ name }),
    }),

  revokeApiKey: (id: string) =>
    request<ActionResponse>(`/api/auth/api-keys/${id}`, {
      method: "DELETE",
    }),

  // Updates
  updateStatus: () => request<UpdateStatus>("/api/updates/status"),

  updateCheck: () =>
    request<ActionResponse>("/api/updates/check", { method: "POST" }),

  updateAll: () =>
    request<ActionResponse>("/api/updates/update-all", { method: "POST" }),

  selfUpdate: () =>
    request<ActionResponse>("/api/updates/self-update", { method: "POST" }),

  updateConfig: () => request<UpdateConfig>("/api/updates/config"),

  saveUpdateConfig: (config: {
    auto_update_apps: boolean;
    auto_update_myground: boolean;
  }) =>
    request<ActionResponse>("/api/updates/config", {
      method: "PUT",
      ...jsonBody(config),
    }),
};
