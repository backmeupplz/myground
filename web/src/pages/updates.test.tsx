import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { Updates } from "./updates";
import { mockFetch, mockFetchPending } from "../test-utils";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Updates", () => {
  it("renders heading", async () => {
    mockFetch({
      "/api/updates/status": {
        myground_version: "1.0.0",
        latest_myground_version: null,
        myground_update_available: false,
        apps: [],
        last_check: null,
      },
      "/api/updates/config": {
        auto_update_apps: false,
        auto_update_myground: false,
        last_check: null,
        latest_myground_version: null,
        latest_myground_url: null,
      },
    });
    render(<Updates />);
    await waitFor(() => {
      expect(screen.getByText("Updates")).toBeTruthy();
    });
  });

  it("shows all apps up to date when no updates", async () => {
    mockFetch({
      "/api/updates/status": {
        myground_version: "1.0.0",
        latest_myground_version: null,
        myground_update_available: false,
        apps: [],
        last_check: null,
      },
      "/api/updates/config": {
        auto_update_apps: false,
        auto_update_myground: false,
        last_check: null,
        latest_myground_version: null,
        latest_myground_url: null,
      },
    });
    render(<Updates />);
    await waitFor(() => {
      expect(screen.getByText("All apps are up to date")).toBeTruthy();
    });
  });

  it("shows app with available update", async () => {
    mockFetch({
      "/api/updates/status": {
        myground_version: "1.0.0",
        latest_myground_version: null,
        myground_update_available: false,
        apps: [{ id: "whoami", name: "Whoami", update_available: true, last_check: null }],
        last_check: null,
      },
      "/api/updates/config": {
        auto_update_apps: false,
        auto_update_myground: false,
        last_check: null,
        latest_myground_version: null,
        latest_myground_url: null,
      },
    });
    render(<Updates />);
    await waitFor(() => {
      expect(screen.getByText("Whoami")).toBeTruthy();
      expect(screen.getByText("Update")).toBeTruthy();
    });
  });

  it("shows MyGround version", async () => {
    mockFetch({
      "/api/updates/status": {
        myground_version: "2.3.4",
        latest_myground_version: null,
        myground_update_available: false,
        apps: [],
        last_check: null,
      },
      "/api/updates/config": {
        auto_update_apps: false,
        auto_update_myground: false,
        last_check: null,
        latest_myground_version: null,
        latest_myground_url: null,
      },
    });
    render(<Updates />);
    await waitFor(() => {
      expect(screen.getByText("v2.3.4")).toBeTruthy();
    });
  });
});
