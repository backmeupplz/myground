import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { AppCard } from "./app-card";
import type { AppInfo } from "../api";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const baseApp: AppInfo = {
  id: "whoami",
  name: "Whoami",
  description: "Simple HTTP app",
  icon: "globe",
  category: "utilities",
  installed: false,
  has_storage: false,
  backup_supported: true,
  containers: [],
  storage: [],
  port: null,
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
  status: "not_installed",
  status_detail: "Not installed",
  ready: false,
  vpn_enabled: false,
  storage_volumes: [],
};

const runningApp: AppInfo = {
  ...baseApp,
  id: "filebrowser",
  name: "File Browser",
  description: "Web-based file manager",
  installed: true,
  containers: [
    { name: "myground-filebrowser", state: "running", status: "Up 2h" },
  ],
  port: 9001,
  status: "running",
  status_detail: "All containers running",
  ready: true,
};

const stoppedApp: AppInfo = {
  ...baseApp,
  id: "immich",
  name: "Immich",
  installed: true,
  containers: [],
  port: 9002,
  status: "stopped",
  status_detail: "All containers stopped",
  ready: false,
};

const noop = () => {};

describe("AppCard", () => {
  it("renders name and description", () => {
    render(
      <AppCard
        app={runningApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("File Browser")).toBeTruthy();
    expect(screen.getByText("Web-based file manager")).toBeTruthy();
  });

  it("shows Running badge for running app", () => {
    render(
      <AppCard
        app={runningApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Running")).toBeTruthy();
  });

  it("shows Open button when ready with domain_url", () => {
    const appWithDomain = { ...runningApp, domain_url: "https://fb.example.com" };
    render(
      <AppCard
        app={appWithDomain}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Open")).toBeTruthy();
  });

  it("shows Open via Tailnet when ready with tailscale_url", () => {
    const appWithTs = { ...runningApp, tailscale_url: "https://myground-fb.tailnet.ts.net", tailscale_disabled: false };
    render(
      <AppCard
        app={appWithTs}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Open via Tailnet")).toBeTruthy();
  });

  it("shows Open via LAN when ready with lan_accessible and serverIp", () => {
    const appWithLan = { ...runningApp, lan_accessible: true };
    render(
      <AppCard
        app={appWithLan}
        onStart={noop}
        onStop={noop}
        busy={false}
        serverIp="192.168.1.10"
      />,
    );
    expect(screen.getByText("Open via LAN")).toBeTruthy();
  });

  it("does not show Open button when not ready", () => {
    render(
      <AppCard
        app={stoppedApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.queryByText("Open")).toBeNull();
    expect(screen.queryByText("Open via Tailnet")).toBeNull();
    expect(screen.queryByText("Open via LAN")).toBeNull();
  });

  it("does not show Open when running but not ready (sidecar connecting)", () => {
    const notReady = { ...runningApp, ready: false, status_detail: "Tailscale sidecar connecting..." };
    render(
      <AppCard
        app={notReady}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.queryByText("Open")).toBeNull();
    expect(screen.queryByText("Open via Tailnet")).toBeNull();
  });

  it("shows Stop button for running app", () => {
    render(
      <AppCard
        app={runningApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Stop")).toBeTruthy();
  });

  it("shows Start and Manage for stopped app", () => {
    render(
      <AppCard
        app={stoppedApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Start")).toBeTruthy();
    expect(screen.getByText("Manage")).toBeTruthy();
  });

  it("calls onStop when Stop clicked", () => {
    const onStop = vi.fn();
    render(
      <AppCard
        app={runningApp}
        onStart={noop}
        onStop={onStop}
        busy={false}
      />,
    );
    screen.getByText("Stop").click();
    expect(onStop).toHaveBeenCalled();
  });

  it("shows deploying status with detail text", () => {
    const deploying = { ...baseApp, installed: true, status: "deploying", status_detail: "Deploying containers...", deploying: true };
    render(
      <AppCard
        app={deploying}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Deploying...")).toBeTruthy();
    expect(screen.getByText("Deploying containers...")).toBeTruthy();
  });

  it("shows crashing status with detail text", () => {
    const crashing = { ...baseApp, installed: true, status: "crashing", status_detail: "Container myground-app restarting" };
    render(
      <AppCard
        app={crashing}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Error")).toBeTruthy();
    expect(screen.getByText("Container myground-app restarting")).toBeTruthy();
  });
});
