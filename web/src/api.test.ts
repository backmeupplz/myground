import { describe, it, expect, afterEach, vi } from "vitest";
import { api } from "./api";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("api", () => {
  it("request parses JSON on success", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ status: "ok", version: "1.0" })),
    );

    const result = await api.health();
    expect(result.status).toBe("ok");
    expect(result.version).toBe("1.0");
  });

  it("request throws on error response", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ message: "Not found" }), { status: 404 }),
    );

    await expect(api.health()).rejects.toThrow("Not found");
  });

  it("api.services calls correct URL", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([])),
    );

    await api.services();
    expect(fetchSpy).toHaveBeenCalledWith("/api/services", undefined);
  });

  it("api.installService calls with POST and JSON body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({ ok: true, message: "installed", port: 9000 }),
      ),
    );

    await api.installService("whoami", { storage_path: "/mnt/data" });
    const [url, opts] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/services/whoami/install");
    expect(opts.method).toBe("POST");
    expect(JSON.parse(opts.body as string)).toEqual({
      storage_path: "/mnt/data",
    });
  });

  it("api.startService calls correct URL with POST", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ ok: true, message: "started" })),
    );

    await api.startService("whoami");
    const [url, opts] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/services/whoami/start");
    expect(opts.method).toBe("POST");
  });

  it("api.removeService calls correct URL with DELETE", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ ok: true, message: "removed" })),
    );

    await api.removeService("whoami");
    const [url, opts] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/services/whoami");
    expect(opts.method).toBe("DELETE");
  });

  it("api.disks calls correct URL", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([])),
    );

    await api.disks();
    expect(fetchSpy).toHaveBeenCalledWith("/api/disks", undefined);
  });
});
