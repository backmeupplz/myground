// ── Types ──────────────────────────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  version: string;
  server_ip?: string;
  tailnet_name?: string;
  available_gpus: string[];
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
  description: string;
  container_path: string;
  host_path: string;
  disk_available_bytes: number | null;
  is_db_dump: boolean;
}

export interface ExtraFolder {
  container_path: string;
  host_path: string;
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
  supports_tailscale: boolean;
  update_available: boolean;
  current_digest?: string | null;
  latest_digest?: string | null;
  domain_url?: string | null;
  supports_gpu: boolean;
  gpu_mode: string | null;
  has_health_check: boolean;
  deploying: boolean;
  status: string;
  status_detail: string;
  ready: boolean;
  vpn_enabled: boolean;
  vpn_provider?: string | null;
  storage_volumes: StorageVolumeInfo[];
  extra_folders?: ExtraFolder[];
  extra_folders_base?: string | null;
}

export interface VpnConfig {
  enabled: boolean;
  provider?: string;
  vpn_type?: string;
  server_countries?: string;
  port_forwarding?: boolean;
  env_vars?: Record<string, string>;
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
  local: BackupConfig[];
  remote: BackupConfig[];
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

export interface StorageVolumeInfo {
  name: string;
  container_path: string;
  description: string;
}

export interface AvailableApp {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  backup_supported: boolean;
  has_health_check: boolean;
  website: string;
  install_variables: InstallVariable[];
  storage_volumes: StorageVolumeInfo[];
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
  default_local_destination?: BackupConfig;
  default_remote_destination?: BackupConfig;
}

export interface BackupJob {
  id: string;
  app_id?: string;
  destination_type: string;
  repository?: string;
  password?: string;
  s3_access_key?: string;
  s3_secret_key?: string;
  schedule?: string;
  last_run_at?: string;
  last_status?: string;
  last_error?: string;
  last_log_lines?: string[];
  last_skipped_at?: string;
}

export interface BackupJobWithApp extends BackupJob {
  app_id: string;
}

export interface BackupJobProgress {
  job_id: string;
  app_id: string;
  status: string;
  percent_done: number;
  seconds_remaining?: number;
  bytes_done: number;
  bytes_total: number;
  current_file?: string;
  error?: string;
  log_lines: string[];
  started_at: string;
}

export interface VerifyResult {
  ok: boolean;
  snapshot_count?: number;
  error?: string;
}

export interface Snapshot {
  id: string;
  time: string;
  paths: string[];
  tags: string[];
  hostname: string;
  source?: string;
}

export interface BackupResult {
  snapshot_id: string;
  files_new: number;
  bytes_added: number;
}

export interface RestoreStartResponse {
  ok: boolean;
  message: string;
  restore_id: string;
}

export interface RestoreProgress {
  restore_id: string;
  snapshot_id: string;
  app_id: string;
  status: string;
  phase: string;
  started_at: string;
  error?: string;
  log_lines: string[];
}

export interface SnapshotFile {
  path: string;
  type: string;
  size: number;
  mtime?: string;
}

export interface TailscaleStatus {
  enabled: boolean;
  exit_node_running: boolean;
  exit_node_approved: boolean | null;
  tailnet: string | null;
  https_enabled: boolean | null;
  pihole_dns: boolean;
  pihole_installed: boolean;
  exit_hostname: string | null;
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
  setup_progress?: string;
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
  name: string;
  update_available: boolean;
  last_check: string | null;
  current_digest?: string | null;
  latest_digest?: string | null;
}

export interface UpdateStatus {
  myground_version: string;
  latest_myground_version: string | null;
  myground_update_available: boolean;
  apps: AppUpdateInfo[];
  last_check: string | null;
}

export interface AwsSetupRequest {
  access_key: string;
  secret_key: string;
  region: string;
}

export interface AwsSetupResult {
  bucket_name: string;
  repository: string;
  s3_access_key: string;
  s3_secret_key: string;
  iam_user_name: string;
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

/**
 * Extract a short display hash (first 12 hex chars) from a repo digest.
 * e.g. "nextcloud@sha256:abc123..." → "abc123..."
 */
export function shortDigest(digest: string): string {
  const hash = digest.split(":").pop() ?? digest;
  return hash.slice(0, 12);
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

/** Format seconds into human-readable duration like "16h 30m" or "2m 15s". */
export function formatEta(seconds: number): string {
  if (seconds < 60) return `${Math.ceil(seconds)}s`;
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60);
    const s = Math.round(seconds % 60);
    return s > 0 ? `${m}m ${s}s` : `${m}m`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
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

  browse: (path = "/", showHidden = false) =>
    request<BrowseResult>(`/api/browse?path=${encodeURIComponent(path)}${showHidden ? "&show_hidden=true" : ""}`),

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

  deployApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/deploy`, { method: "POST" }),

  startApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/start`, { method: "POST" }),

  stopApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}/stop`, { method: "POST" }),

  removeApp: (id: string) =>
    request<ActionResponse>(`/api/apps/${id}`, { method: "DELETE" }),

  disks: () => request<DiskInfo[]>("/api/disks"),

  getAppBackup: async (id: string) => {
    const cfg = await request<AppBackupConfig>(`/api/apps/${id}/backup`);
    cfg.local = cfg.local ?? [];
    cfg.remote = cfg.remote ?? [];
    return cfg;
  },

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

  getBackupPassword: (id: string) =>
    request<{ password: string | null }>(`/api/apps/${id}/backup-password`),

  backupRunAll: () =>
    request<BackupResult[]>("/api/backup/run", { method: "POST" }),

  backupSnapshots: () => request<Snapshot[]>("/api/backup/snapshots"),

  backupRestore: (snapshotId: string, targetPath: string) =>
    request<RestoreStartResponse>(`/api/backup/restore/${snapshotId}`, {
      method: "POST",
      ...jsonBody({ target_path: targetPath }),
    }),

  backupRestoreDb: (snapshotId: string) =>
    request<RestoreStartResponse>(`/api/backup/restore/${snapshotId}`, {
      method: "POST",
      ...jsonBody({}),
    }),

  restoreProgress: (restoreId: string) =>
    request<RestoreProgress>(`/api/backup/restore/${restoreId}/progress`),

  activeRestores: () =>
    request<RestoreProgress[]>("/api/backup/restores"),

  appBackupSnapshots: (id: string) =>
    request<Snapshot[]>(`/api/apps/${id}/backup/snapshots`),

  appBackupRun: (id: string) =>
    request<BackupResult[]>(`/api/apps/${id}/backup/run`, {
      method: "POST",
    }),

  backupConfig: () => request<BackupConfig>("/api/backup/config"),

  awsSetup: (body: AwsSetupRequest) =>
    request<AwsSetupResult>("/api/backup/aws-setup", {
      method: "POST",
      ...jsonBody(body),
    }),

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
    pihole_dns?: boolean;
    exit_hostname?: string | null;
  }) =>
    request<ActionResponse>("/api/tailscale/config", {
      method: "PUT",
      ...jsonBody(body),
    }),

  tailscaleRefresh: () =>
    request<ActionResponse>("/api/tailscale/refresh", { method: "POST" }),

  togglePiholeDns: (
    enable: boolean,
    onLog: (line: string) => void,
  ): Promise<boolean> => {
    return new Promise((resolve) => {
      let resolved = false;
      let pollId: ReturnType<typeof setInterval> | undefined;
      const done = (ok: boolean) => {
        if (resolved) return;
        resolved = true;
        clearInterval(pollId);
        resolve(ok);
      };
      // Poll status API every 3s as a fallback — if the WebSocket dies
      // (e.g. exit node restart kills the Tailscale tunnel), the poller
      // detects completion within seconds instead of hanging forever.
      const startPolling = () => {
        pollId = setInterval(() => {
          api.tailscaleStatus().then((s) => {
            const succeeded = enable ? s.pihole_dns : !s.pihole_dns;
            if (succeeded) {
              onLog(enable ? "Pi-hole DNS enabled" : "Pi-hole DNS disabled");
              ws.close();
              done(true);
            }
          }).catch((e) => console.warn("Tailscale poll error:", e)); // best-effort while tunnel is reconnecting
        }, 3_000);
      };
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      const ws = new WebSocket(`${proto}//${location.host}/api/tailscale/pihole-dns`);
      ws.onopen = () => {
        ws.send(JSON.stringify({ enable }));
        startPolling();
      };
      ws.onmessage = (e) => {
        const msg = e.data as string;
        if (msg === "__DONE__") {
          ws.close();
          done(true);
        } else if (msg.startsWith("Error:")) {
          onLog(msg);
          ws.close();
          done(false);
        } else {
          onLog(msg);
        }
      };
      ws.onerror = () => {
        onLog("Connection error");
        done(false);
      };
      ws.onclose = () => {
        // Don't resolve on close — let the poller handle it
      };
    });
  },

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

  setExtraFolders: (id: string, folders: ExtraFolder[]) =>
    request<ActionResponse>(`/api/apps/${id}/folders`, {
      method: "PUT",
      ...jsonBody({ folders }),
    }),

  // VPN
  getAppVpn: (id: string) =>
    request<VpnConfig>(`/api/apps/${id}/vpn`),

  setAppVpn: (id: string, config: VpnConfig) =>
    request<ActionResponse>(`/api/apps/${id}/vpn`, {
      method: "PUT",
      ...jsonBody(config),
    }),

  getVpnConfig: () => request<VpnConfig>("/api/vpn/config"),

  saveVpnConfig: (config: VpnConfig) =>
    request<ActionResponse>("/api/vpn/config", {
      method: "PUT",
      ...jsonBody(config),
    }),

  testVpn: (
    config: VpnConfig | undefined,
    onLog: (line: string) => void,
  ): Promise<boolean> => {
    return new Promise((resolve) => {
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      const ws = new WebSocket(`${proto}//${location.host}/api/vpn/test`);
      ws.onopen = () => {
        ws.send(JSON.stringify(config ?? {}));
      };
      ws.onmessage = (e) => {
        const msg = e.data as string;
        if (msg === "__DONE__") {
          ws.close();
          resolve(true);
        } else if (msg.startsWith("__FAIL__")) {
          ws.close();
          resolve(false);
        } else {
          onLog(msg);
        }
      };
      ws.onerror = () => {
        resolve(false);
      };
      ws.onclose = () => {};
    });
  },

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

  // Backup Jobs
  backupJobs: () => request<BackupJobWithApp[]>("/api/backup/jobs"),

  createBackupJob: (body: {
    app_id: string;
    destination_type: string;
    repository?: string;
    password?: string;
    s3_access_key?: string;
    s3_secret_key?: string;
    schedule?: string;
  }) =>
    request<BackupJob>("/api/backup/jobs", {
      method: "POST",
      ...jsonBody(body),
    }),

  updateBackupJob: (
    id: string,
    body: {
      repository?: string;
      password?: string;
      s3_access_key?: string;
      s3_secret_key?: string;
      schedule?: string;
      destination_type?: string;
    },
  ) =>
    request<ActionResponse>(`/api/backup/jobs/${id}`, {
      method: "PUT",
      ...jsonBody(body),
    }),

  deleteBackupJob: (id: string) =>
    request<ActionResponse>(`/api/backup/jobs/${id}`, {
      method: "DELETE",
    }),

  runBackupJob: (id: string) =>
    request<ActionResponse>(`/api/backup/jobs/${id}/run`, {
      method: "POST",
    }),

  cancelBackupJob: (id: string) =>
    request<ActionResponse>(`/api/backup/jobs/${id}/cancel`, {
      method: "POST",
    }),

  backupJobProgress: (id: string) =>
    request<BackupJobProgress>(`/api/backup/jobs/${id}/progress`),

  snapshotFiles: (id: string, path?: string) => {
    const params = path ? `?path=${encodeURIComponent(path)}` : "";
    return request<SnapshotFile[]>(`/api/backup/snapshots/${id}/files${params}`);
  },

  deleteSnapshot: (id: string) =>
    request<ActionResponse>(`/api/backup/snapshots/${id}`, {
      method: "DELETE",
    }),

  verifyBackup: (config: BackupConfig) =>
    request<VerifyResult>("/api/backup/verify", {
      method: "POST",
      ...jsonBody(config),
    }),
};
