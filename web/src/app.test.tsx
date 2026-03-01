import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { App } from "./app";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** Mock fetch to simulate an authenticated session. */
function mockAuthed(health: object) {
  vi.spyOn(globalThis, "fetch").mockImplementation((url) => {
    const path = typeof url === "string" ? url : (url as Request).url;
    if (path.includes("/api/auth/status")) {
      return Promise.resolve(
        new Response(JSON.stringify({ setup_required: false, authenticated: true })),
      );
    }
    if (path.includes("/api/health")) {
      return Promise.resolve(new Response(JSON.stringify(health)));
    }
    // Default: empty services list
    return Promise.resolve(new Response(JSON.stringify([])));
  });
}

describe("App", () => {
  it("shows connecting state initially", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
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
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ setup_required: true, authenticated: false })),
    );
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText("MyGround Setup")).toBeTruthy();
    });
  });

  it("shows version after authenticated", async () => {
    mockAuthed({ status: "ok", version: "1.2.3" });
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
