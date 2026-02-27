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
    containers: [],
    storage: [],
    port: null,
  },
  {
    id: "filebrowser",
    name: "File Browser",
    description: "Web-based file manager",
    icon: "folder",
    category: "files",
    installed: true,
    containers: [
      { name: "myground-filebrowser", state: "running", status: "Up 2h" },
    ],
    storage: [],
    port: 9001,
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

  it("fetches and renders service grid", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("Whoami")).toBeTruthy();
      expect(screen.getByText("File Browser")).toBeTruthy();
    });
  });

  it("shows Add Service card", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("+ Add Service")).toBeTruthy();
    });
  });

  it("shows correct status badges", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText("Not Installed")).toBeTruthy();
      expect(screen.getByText("Running")).toBeTruthy();
    });
  });
});
