import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { AppCard, getAppStatus } from "./app-card";
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
};

const stoppedApp: AppInfo = {
  ...baseApp,
  id: "immich",
  name: "Immich",
  installed: true,
  containers: [],
  port: 9002,
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

  it("shows Open button only when running with port", () => {
    render(
      <AppCard
        app={runningApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Open")).toBeTruthy();
  });

  it("does not show Open button when not running", () => {
    render(
      <AppCard
        app={stoppedApp}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.queryByText("Open")).toBeNull();
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
});

describe("getAppStatus", () => {
  it("returns not_installed for uninstalled app", () => {
    expect(getAppStatus(baseApp)).toBe("not_installed");
  });

  it("returns running for running app", () => {
    expect(getAppStatus(runningApp)).toBe("running");
  });

  it("returns stopped for installed but no running containers", () => {
    expect(getAppStatus(stoppedApp)).toBe("stopped");
  });
});
