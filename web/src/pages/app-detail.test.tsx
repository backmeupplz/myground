import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { AppDetail } from "./app-detail";
import type { AppInfo } from "../api";

const mockApp: AppInfo = {
  id: "filebrowser",
  name: "File Browser",
  description: "Web-based file manager",
  icon: "folder",
  category: "files",
  installed: true,
  has_storage: true,
  backup_supported: true,
  containers: [
    { name: "myground-filebrowser", state: "running", status: "Up 2h" },
  ],
  storage: [
    {
      name: "data",
      description: "Application data",
      container_path: "/srv",
      host_path: "/mnt/data/fb",
      disk_available_bytes: 50000000000,
      is_db_dump: false,
    },
  ],
  port: 9001,
  install_variables: [],
  env_overrides: {},
  has_backup_password: false,
  tailscale_disabled: false,
  lan_accessible: false,
  uses_host_network: false,
  supports_tailscale: false,
  update_available: false,
  supports_gpu: false,
  gpu_mode: null,
  has_health_check: false,
  deploying: false,
  status: "running",
  status_detail: "All containers running",
  ready: true,
  vpn_enabled: false,
  storage_volumes: [],
};

function mockFetch(apps: AppInfo[]) {
  vi.spyOn(globalThis, "fetch").mockImplementation((url) => {
    const path = typeof url === "string" ? url : (url as Request).url;
    if (path.includes("/api/health")) {
      return Promise.resolve(new Response(JSON.stringify({ status: "ok", version: "0.1.0", server_ip: "192.168.1.10" })));
    }
    if (path.includes("/api/cloudflare/status")) {
      return Promise.resolve(new Response(JSON.stringify({ enabled: false, tunnel_running: false, tunnel_id: null, bindings: [] })));
    }
    if (path.includes("/api/vpn/config")) {
      return Promise.resolve(new Response(JSON.stringify({ enabled: false })));
    }
    // Default: /api/apps
    return Promise.resolve(new Response(JSON.stringify(apps)));
  });
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("AppDetail", () => {
  it("renders app name and status", async () => {
    mockFetch([mockApp]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("File Browser")).toBeTruthy();
      expect(screen.getByText("Running")).toBeTruthy();
    });
  });

  it("renders Stop button for running app", async () => {
    mockFetch([mockApp]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Stop")).toBeTruthy();
    });
  });

  it("renders Open button when app is ready with domain_url", async () => {
    const appWithDomain = { ...mockApp, domain_url: "https://fb.example.com" };
    mockFetch([appWithDomain]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Open")).toBeTruthy();
    });
  });

  it("renders Open via LAN when app is ready with lan_accessible", async () => {
    const appWithLan = { ...mockApp, lan_accessible: true };
    mockFetch([appWithLan]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Open via LAN")).toBeTruthy();
    });
  });

  it("does not render Open button when app is not ready", async () => {
    const notReady = { ...mockApp, ready: false };
    mockFetch([notReady]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Stop")).toBeTruthy();
    });
    expect(screen.queryByText("Open")).toBeNull();
    expect(screen.queryByText("Open via Tailnet")).toBeNull();
    expect(screen.queryByText("Open via LAN")).toBeNull();
  });

  it("renders storage info", async () => {
    mockFetch([mockApp]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Application data")).toBeTruthy();
      expect(screen.getByText(/free/)).toBeTruthy();
    });
  });

  it("shows not found for unknown app", async () => {
    mockFetch([]);

    render(<AppDetail id="nonexistent" />);

    await waitFor(() => {
      expect(screen.getByText("App not found.")).toBeTruthy();
    });
  });

  it("shows backup config section", async () => {
    mockFetch([mockApp]);

    render(<AppDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Backup")).toBeTruthy();
    });
  });
});
