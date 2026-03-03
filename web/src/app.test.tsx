import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { App } from "./app";
import { mockFetch, mockFetchPending } from "./test-utils";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("App", () => {
  it("shows connecting state initially", () => {
    mockFetchPending();
    render(<App />);
    expect(screen.getByText("Connecting...")).toBeTruthy();
  });

  it("shows login page when not authenticated", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ setup_required: false, authenticated: false })),
    );
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText("Sign in to continue.")).toBeTruthy();
    });
  });

  it("shows setup page when setup required", async () => {
    mockFetch({
      "/api/auth/status": { setup_required: true, authenticated: false },
    });
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText("Welcome to MyGround")).toBeTruthy();
    });
  });

  it("shows version after authenticated", async () => {
    mockFetch({
      "/api/auth/status": { setup_required: false, authenticated: true },
      "/api/health": { status: "ok", version: "1.2.3" },
      "/api/updates/status": { myground_version: "1.2.3", latest_myground_version: null, myground_update_available: false, apps: [], last_check: null },
    });
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText(/v1\.2\.3/)).toBeTruthy();
    });
  });

  it("fetches auth status on mount", () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
    render(<App />);
    expect(fetchSpy).toHaveBeenCalledWith("/api/auth/status", undefined);
  });

  it("shows connecting state when health fetch fails", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(
      new Error("network error"),
    );
    render(<App />);
    await waitFor(() => {
      // Falls back to login state when auth check fails
      expect(screen.getByText("Sign in to continue.")).toBeTruthy();
    });
  });
});
