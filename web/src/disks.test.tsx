import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { DiskList, type DiskInfo } from "./disks";

const mockDisks: DiskInfo[] = [
  {
    name: "sda1",
    mount_point: "/",
    total_bytes: 500 * 1024 ** 3, // 500 GB
    available_bytes: 200 * 1024 ** 3, // 200 GB
    used_bytes: 300 * 1024 ** 3, // 300 GB
    fs_type: "ext4",
    is_removable: false,
  },
  {
    name: "sdb1",
    mount_point: "/mnt/external",
    total_bytes: 2 * 1024 ** 4, // 2 TB
    available_bytes: 1.5 * 1024 ** 4,
    used_bytes: 0.5 * 1024 ** 4,
    fs_type: "btrfs",
    is_removable: true,
  },
];

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DiskList", () => {
  it("shows loading state initially", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
    render(<DiskList />);
    expect(screen.getByText("Loading disks...")).toBeTruthy();
  });

  it("renders disk cards after fetch", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockDisks)),
    );

    render(<DiskList />);

    await waitFor(() => {
      expect(screen.getByText("/")).toBeTruthy();
      expect(screen.getByText("/mnt/external")).toBeTruthy();
    });
  });

  it("shows filesystem type", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockDisks)),
    );

    render(<DiskList />);

    await waitFor(() => {
      expect(screen.getByText(/ext4/)).toBeTruthy();
      expect(screen.getByText(/btrfs/)).toBeTruthy();
    });
  });

  it("shows removable badge for removable disks", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockDisks)),
    );

    render(<DiskList />);

    await waitFor(() => {
      expect(screen.getByText(/Removable/)).toBeTruthy();
    });
  });

  it("formats bytes correctly in size display", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockDisks)),
    );

    render(<DiskList />);

    await waitFor(() => {
      // 200 GB free of 500 GB (root disk)
      expect(screen.getByText(/200\.0 GB free/)).toBeTruthy();
      expect(screen.getByText(/500\.0 GB/)).toBeTruthy();
    });
  });

  it("shows usage percentage", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockDisks)),
    );

    render(<DiskList />);

    await waitFor(() => {
      // 300/500 = 60%
      expect(screen.getByText("60% used")).toBeTruthy();
      // 0.5/2 = 25%
      expect(screen.getByText("25% used")).toBeTruthy();
    });
  });

  it("shows empty state when no disks", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([])),
    );

    render(<DiskList />);

    await waitFor(() => {
      expect(screen.getByText("No disks found.")).toBeTruthy();
    });
  });

  it("shows empty state on fetch error", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network"));

    render(<DiskList />);

    await waitFor(() => {
      expect(screen.getByText("No disks found.")).toBeTruthy();
    });
  });

  it("fetches /api/disks on mount", () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockDisks)),
    );

    render(<DiskList />);

    expect(fetchSpy).toHaveBeenCalledWith("/api/disks");
  });
});
