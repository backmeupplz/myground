import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup, waitFor } from "@testing-library/preact";
import { Settings } from "./settings";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Settings", () => {
  it("shows loading state initially", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
    render(<Settings />);
    expect(screen.getByText("Loading settings...")).toBeTruthy();
  });

  it("renders settings form after loading", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          version: "0.1.0",
          default_storage_path: null,
          backup: null,
        }),
      ),
    );

    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByText("Settings")).toBeTruthy();
      expect(screen.getByText("Default Storage Path")).toBeTruthy();
      expect(screen.getByText("Global Backup Defaults")).toBeTruthy();
    });
  });

  it("shows save button", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({ version: "0.1.0" }),
      ),
    );

    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByText("Save")).toBeTruthy();
    });
  });

  it("displays current storage path", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          version: "0.1.0",
          default_storage_path: "/mnt/data",
        }),
      ),
    );

    render(<Settings />);

    await waitFor(() => {
      expect(screen.getByText("/mnt/data")).toBeTruthy();
    });
  });

  it("shows default placeholder when no storage path set", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({ version: "0.1.0" }),
      ),
    );

    render(<Settings />);

    await waitFor(() => {
      expect(
        screen.getByText("~/.myground/services/ (default)"),
      ).toBeTruthy();
    });
  });
});
