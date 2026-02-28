import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, waitFor, cleanup } from "@testing-library/preact";
import { ServiceDetail } from "./service-detail";
import type { ServiceInfo } from "../api";

const mockService: ServiceInfo = {
  id: "filebrowser",
  name: "File Browser",
  description: "Web-based file manager",
  icon: "folder",
  category: "files",
  installed: true,
  has_storage: true,
  backup_supported: true,
  containers: [
    { name: "myground-filebrowser", state: "running", status: "Up 2h" },
  ],
  storage: [
    {
      name: "data",
      container_path: "/srv",
      host_path: "/mnt/data/fb",
      disk_available_bytes: 50000000000,
    },
  ],
  port: 9001,
  install_variables: [],
  env_overrides: {},
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ServiceDetail", () => {
  it("renders service name and status", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([mockService])),
    );

    render(<ServiceDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("File Browser")).toBeTruthy();
      expect(screen.getByText("Running")).toBeTruthy();
    });
  });

  it("renders action buttons", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([mockService])),
    );

    render(<ServiceDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Open")).toBeTruthy();
      expect(screen.getByText("Stop")).toBeTruthy();
    });
  });

  it("renders storage info", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([mockService])),
    );

    render(<ServiceDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("data")).toBeTruthy();
      expect(screen.getByText(/free/)).toBeTruthy();
    });
  });

  it("shows not found for unknown service", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([])),
    );

    render(<ServiceDetail id="nonexistent" />);

    await waitFor(() => {
      expect(screen.getByText("Service not found.")).toBeTruthy();
    });
  });

  it("shows backup config section", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([mockService])),
    );

    render(<ServiceDetail id="filebrowser" />);

    await waitFor(() => {
      expect(screen.getByText("Backup")).toBeTruthy();
    });
  });
});
