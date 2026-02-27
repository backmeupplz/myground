import { describe, it, expect, afterEach, vi, beforeEach } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { ServiceList, type ServiceInfo } from "./services";

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
  },
  {
    id: "filebrowser",
    name: "File Browser",
    description: "Web-based file manager",
    icon: "folder",
    category: "files",
    installed: true,
    containers: [{ name: "myground-filebrowser", state: "running", status: "Up 2 hours" }],
    storage: [
      { name: "data", container_path: "/srv", host_path: "/home/user/.myground/services/filebrowser/volumes/data", disk_available_bytes: 50000000000 },
      { name: "config", container_path: "/config", host_path: "/home/user/.myground/services/filebrowser/volumes/config", disk_available_bytes: 50000000000 },
    ],
  },
  {
    id: "immich",
    name: "Immich",
    description: "Photo management",
    icon: "image",
    category: "photos",
    installed: true,
    containers: [],
    storage: [
      { name: "upload", container_path: "/usr/src/app/upload", host_path: "/mnt/photos", disk_available_bytes: 100000000000 },
    ],
  },
];

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ServiceList", () => {
  it("shows loading state initially", () => {
    vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
    render(<ServiceList />);
    expect(screen.getByText("Loading services...")).toBeTruthy();
  });

  it("renders service cards after fetch", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Whoami")).toBeTruthy();
      expect(screen.getByText("File Browser")).toBeTruthy();
      expect(screen.getByText("Immich")).toBeTruthy();
    });
  });

  it("shows correct status badges", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Not Installed")).toBeTruthy();
      expect(screen.getByText("Running")).toBeTruthy();
      expect(screen.getByText("Stopped")).toBeTruthy();
    });
  });

  it("shows install button for not-installed services", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Install")).toBeTruthy();
    });
  });

  it("shows stop button for running services", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Stop")).toBeTruthy();
    });
  });

  it("shows start and remove buttons for stopped services", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Start")).toBeTruthy();
      expect(screen.getByText("Remove")).toBeTruthy();
    });
  });

  it("shows storage paths for installed services", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify(mockServices)),
    );

    render(<ServiceList />);

    await waitFor(() => {
      // filebrowser has "data" and "config" storage
      expect(screen.getByText(/data:/)).toBeTruthy();
      expect(screen.getByText(/config:/)).toBeTruthy();
      // immich has "upload" storage
      expect(screen.getByText(/upload:/)).toBeTruthy();
    });
  });

  it("shows empty services list gracefully", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([])),
    );

    render(<ServiceList />);

    await waitFor(() => {
      // Should not show loading anymore
      expect(screen.queryByText("Loading services...")).toBeNull();
    });
  });

  it("calls POST /api/services/{id}/install on install click", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(JSON.stringify(mockServices)))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, message: "installed" })))
      .mockResolvedValue(new Response(JSON.stringify(mockServices)));

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Install")).toBeTruthy();
    });

    const installBtn = screen.getByText("Install");
    installBtn.click();

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith("/api/services/whoami/install", { method: "POST" });
    });
  });

  it("calls POST /api/services/{id}/stop on stop click", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(JSON.stringify(mockServices)))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, message: "stopped" })))
      .mockResolvedValue(new Response(JSON.stringify(mockServices)));

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Stop")).toBeTruthy();
    });

    screen.getByText("Stop").click();

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith("/api/services/filebrowser/stop", { method: "POST" });
    });
  });

  it("calls POST /api/services/{id}/start on start click", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(JSON.stringify(mockServices)))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, message: "started" })))
      .mockResolvedValue(new Response(JSON.stringify(mockServices)));

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Start")).toBeTruthy();
    });

    screen.getByText("Start").click();

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith("/api/services/immich/start", { method: "POST" });
    });
  });

  it("calls DELETE /api/services/{id} on remove click", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(new Response(JSON.stringify(mockServices)))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true, message: "removed" })))
      .mockResolvedValue(new Response(JSON.stringify(mockServices)));

    render(<ServiceList />);

    await waitFor(() => {
      expect(screen.getByText("Remove")).toBeTruthy();
    });

    screen.getByText("Remove").click();

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalledWith("/api/services/immich", { method: "DELETE" });
    });
  });

  it("handles fetch error gracefully", async () => {
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new Error("network"));

    render(<ServiceList />);

    // Should stop loading even on error
    await waitFor(() => {
      expect(screen.queryByText("Loading services...")).toBeNull();
    });
  });
});
