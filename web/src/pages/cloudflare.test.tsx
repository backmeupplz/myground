import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { Cloudflare } from "./cloudflare";
import { mockFetch, mockFetchPending } from "../test-utils";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Cloudflare", () => {
  it("shows loading state initially", () => {
    mockFetchPending();
    render(<Cloudflare />);
    expect(screen.getByText("Loading...")).toBeTruthy();
  });

  it("shows setup form when disabled", async () => {
    mockFetch({
      "/api/cloudflare/status": {
        enabled: false,
        tunnel_running: false,
        tunnel_id: null,
        bindings: [],
      },
    });
    render(<Cloudflare />);
    await waitFor(() => {
      expect(screen.getByText("Cloudflare")).toBeTruthy();
      expect(screen.getByText("Disabled")).toBeTruthy();
      expect(screen.getByText("Enable")).toBeTruthy();
    });
  });

  it("shows connected status when enabled", async () => {
    mockFetch({
      "/api/cloudflare/status": {
        enabled: true,
        tunnel_running: true,
        tunnel_id: "abc-123",
        bindings: [],
      },
    });
    render(<Cloudflare />);
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeTruthy();
      expect(screen.getByText("Running")).toBeTruthy();
    });
  });
});
