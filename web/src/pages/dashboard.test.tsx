import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { Dashboard } from "./dashboard";
import type { ServiceInfo } from "../api";

const mockServices: ServiceInfo[] = [
  {
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
  },
  {
    id: "filebrowser",
    name: "File Browser",
    description: "Web-based file manager",
    icon: "folder",
    category: "files",
    installed: true,
    has_storage: false,
    backup_supported: false,
    containers: [
      { name: "myground-filebrowser", state: "running", status: "Up 2h" },
    ],
    storage: [],
    port: 9001,
    install_variables: [],
    env_overrides: {},
  },
];

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Dashboard", () => {
  it("shows loading state initially", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
    render(<Dashboard />);
    expect(screen.getByText("Loading services...")).toBeTruthy();
  });

  it("shows only installed services and add button", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("File Browser")).toBeTruthy();
      expect(screen.getByText("Add Service")).toBeTruthy();
      // Not-installed services should NOT appear
      expect(screen.queryByText("Whoami")).toBeNull();
    });
  });

  it("shows correct status badges for installed services", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("Running")).toBeTruthy();
      // Not Installed badge should not appear
      expect(screen.queryByText("Not Installed")).toBeNull();
    });
  });
});
