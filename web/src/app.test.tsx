import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { App } from "./app";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("App", () => {
  it("renders the title", () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ status: "ok", version: "0.1.0" })),
    );
    render(<App />);
    expect(screen.getByText("MyGround")).toBeTruthy();
  });

  it("renders the tagline", () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ status: "ok", version: "0.1.0" })),
    );
    render(<App />);
    expect(screen.getByText("Hold your ground.")).toBeTruthy();
  });

  it("shows connecting state initially", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
    render(<App />);
    expect(screen.getByText("Connecting...")).toBeTruthy();
  });

  it("shows server status after health fetch succeeds", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ status: "ok", version: "1.2.3" })),
    );

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText(/Server: ok/)).toBeTruthy();
      expect(screen.getByText(/v1\.2\.3/)).toBeTruthy();
    });
  });

  it("fetches /api/health on mount", () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(
        new Response(JSON.stringify({ status: "ok", version: "0.1.0" })),
      );

    render(<App />);

    expect(fetchSpy).toHaveBeenCalledWith("/api/health");
  });

  it("shows connecting state when health fetch fails", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(
      new Error("network error"),
    );

    render(<App />);

    // Wait for the rejected promise to settle, then check we're still showing Connecting
    await waitFor(() => {
      expect(screen.getByText("Connecting...")).toBeTruthy();
    });
  });
});
