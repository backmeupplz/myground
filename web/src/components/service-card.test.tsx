import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@testing-library/preact";
import { ServiceCard, getServiceStatus } from "./service-card";
import type { ServiceInfo } from "../api";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const baseService: ServiceInfo = {
  id: "whoami",
  name: "Whoami",
  description: "Simple HTTP service",
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

const runningService: ServiceInfo = {
  ...baseService,
  id: "filebrowser",
  name: "File Browser",
  description: "Web-based file manager",
  installed: true,
  containers: [
    { name: "myground-filebrowser", state: "running", status: "Up 2h" },
  ],
  port: 9001,
};

const stoppedService: ServiceInfo = {
  ...baseService,
  id: "immich",
  name: "Immich",
  installed: true,
  containers: [],
  port: 9002,
};

const noop = () => {};

describe("ServiceCard", () => {
  it("renders name and description", () => {
    render(
      <ServiceCard
        service={runningService}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("File Browser")).toBeTruthy();
    expect(screen.getByText("Web-based file manager")).toBeTruthy();
  });

  it("shows Running badge for running service", () => {
    render(
      <ServiceCard
        service={runningService}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Running")).toBeTruthy();
  });

  it("shows Open button only when running with port", () => {
    render(
      <ServiceCard
        service={runningService}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Open")).toBeTruthy();
  });

  it("does not show Open button when not running", () => {
    render(
      <ServiceCard
        service={stoppedService}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.queryByText("Open")).toBeNull();
  });

  it("shows Stop button for running service", () => {
    render(
      <ServiceCard
        service={runningService}
        onStart={noop}
        onStop={noop}
        busy={false}
      />,
    );
    expect(screen.getByText("Stop")).toBeTruthy();
  });

  it("shows Start and Manage for stopped service", () => {
    render(
      <ServiceCard
        service={stoppedService}
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
      <ServiceCard
        service={runningService}
        onStart={noop}
        onStop={onStop}
        busy={false}
      />,
    );
    screen.getByText("Stop").click();
    expect(onStop).toHaveBeenCalled();
  });
});

describe("getServiceStatus", () => {
  it("returns not_installed for uninstalled service", () => {
    expect(getServiceStatus(baseService)).toBe("not_installed");
  });

  it("returns running for running service", () => {
    expect(getServiceStatus(runningService)).toBe("running");
  });

  it("returns stopped for installed but no running containers", () => {
    expect(getServiceStatus(stoppedService)).toBe("stopped");
  });
});
