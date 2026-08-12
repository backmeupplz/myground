import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { Tailscale } from "./tailscale";
import { mockFetch, mockFetchPending } from "../test-utils";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Tailscale", () => {
  it("shows loading state initially", () => {
    mockFetchPending();
    render(<Tailscale />);
    expect(screen.getByText("Loading...")).toBeTruthy();
  });

  it("shows setup form when disabled", async () => {
    mockFetch({
      "/api/tailscale/status": {
        enabled: false,
        provider: "tailscale",
        login_server: null,
        exit_node_running: false,
        exit_node_approved: null,
        tailnet: null,
        apps: [],
      },
    });
    render(<Tailscale />);
    await waitFor(() => {
      expect(screen.getByText("Private Network")).toBeTruthy();
      expect(screen.getByText("Disabled")).toBeTruthy();
      expect(screen.getByText("Enable")).toBeTruthy();
    });
  });

  it("shows enabled status with exit node running", async () => {
    mockFetch({
      "/api/tailscale/status": {
        enabled: true,
        provider: "tailscale",
        login_server: null,
        exit_node_running: true,
        exit_node_approved: true,
        tailnet: "my-tailnet.ts.net",
        apps: [],
      },
    });
    render(<Tailscale />);
    await waitFor(() => {
      expect(screen.getByText("Tailscale")).toBeTruthy();
      expect(screen.getByText("Running")).toBeTruthy();
    });
  });

  it("offers Headscale without removing the Tailscale choice", async () => {
    mockFetch({
      "/api/tailscale/status": {
        enabled: false,
        provider: "tailscale",
        login_server: null,
        exit_node_running: false,
        exit_node_approved: null,
        tailnet: null,
        apps: [],
      },
    });
    render(<Tailscale />);
    await waitFor(() => expect(screen.getByText("Headscale")).toBeTruthy());
    expect(screen.getByText("Tailscale")).toBeTruthy();
  });
});
