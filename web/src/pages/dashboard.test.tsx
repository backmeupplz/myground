import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { Dashboard } from "./dashboard";
import type { AppInfo } from "../api";
import { mockFetchPending } from "../test-utils";

const mockApps: AppInfo[] = [
  {
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
    mockFetchPending();
    render(<Dashboard />);
    expect(screen.getByText("Loading apps...")).toBeTruthy();
  });

  it("shows only installed apps and add button", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockApps)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("File Browser")).toBeTruthy();
      expect(screen.getByText("Add App")).toBeTruthy();
      // Not-installed apps should NOT appear
      expect(screen.queryByText("Whoami")).toBeNull();
    });
  });

  it("shows correct status badges for installed apps", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockApps)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("Running")).toBeTruthy();
      // Not Installed badge should not appear
      expect(screen.queryByText("Not Installed")).toBeNull();
    });
  });
});
