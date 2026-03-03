import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { Backups } from "./backups";
import { mockFetch, mockFetchPending } from "../test-utils";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Backups", () => {
  it("shows loading state initially", () => {
    mockFetchPending();
    render(<Backups />);
    expect(screen.getByText("Loading...")).toBeTruthy();
  });

  it("shows no apps message when none are backup-eligible", async () => {
    mockFetch({
      "/api/apps": [],
    });
    render(<Backups />);
    await waitFor(() => {
      expect(screen.getByText("No installed apps with backup support.")).toBeTruthy();
    });
  });

  it("renders heading after load", async () => {
    mockFetch({
      "/api/apps": [],
    });
    render(<Backups />);
    await waitFor(() => {
      expect(screen.getByText("Backups")).toBeTruthy();
    });
  });
});
