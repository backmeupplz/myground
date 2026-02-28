import { describe, it, expect, afterEach, vi } from "vitest";
import {
  api,
  generatePassword,
  containerColor,
  containerIcon,
  isReady,
  isCrashLooping,
  formatBytes,
} from "./api";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("generatePassword", () => {
  it("returns string of correct length", () => {
    expect(generatePassword(16)).toHaveLength(16);
    expect(generatePassword(32)).toHaveLength(32);
  });

  it("returns different values each call", () => {
    expect(generatePassword(32)).not.toBe(generatePassword(32));
  });
});

describe("containerColor", () => {
  it("returns green for running", () => {
    expect(containerColor({ name: "c", state: "running", status: "" })).toBe(
      "text-green-400",
    );
  });

  it("returns gray for created", () => {
    expect(containerColor({ name: "c", state: "created", status: "" })).toBe(
      "text-gray-400",
    );
  });

  it("returns red for other states", () => {
    expect(containerColor({ name: "c", state: "exited", status: "" })).toBe(
      "text-red-400",
    );
  });
});

describe("containerIcon", () => {
  it("returns checkmark for running", () => {
    expect(containerIcon({ name: "c", state: "running", status: "" })).toBe(
      "\u2713",
    );
  });

  it("returns circle for non-running", () => {
    expect(containerIcon({ name: "c", state: "exited", status: "" })).toBe(
      "\u25cb",
    );
  });
});

describe("isReady", () => {
  it("returns false for empty containers", () => {
    expect(isReady([])).toBe(false);
  });

  it("returns true when all running", () => {
    expect(
      isReady([
        { name: "a", state: "running", status: "" },
        { name: "b", state: "running", status: "" },
      ]),
    ).toBe(true);
  });

  it("returns false when one not running", () => {
    expect(
      isReady([
        { name: "a", state: "running", status: "" },
        { name: "b", state: "exited", status: "" },
      ]),
    ).toBe(false);
  });
});

describe("isCrashLooping", () => {
  it("returns false for healthy containers", () => {
    expect(
      isCrashLooping([{ name: "a", state: "running", status: "Up 5 min" }]),
    ).toBe(false);
  });

  it("detects restarting", () => {
    expect(
      isCrashLooping([
        { name: "a", state: "running", status: "Restarting (1) 5s ago" },
      ]),
    ).toBe(true);
  });

  it("detects exited state", () => {
    expect(
      isCrashLooping([{ name: "a", state: "exited", status: "" }]),
    ).toBe(true);
  });

  it("detects dead state", () => {
    expect(
      isCrashLooping([{ name: "a", state: "dead", status: "" }]),
    ).toBe(true);
  });
});

describe("formatBytes", () => {
  it("formats bytes", () => {
    expect(formatBytes(500)).toBe("500 B");
  });

  it("formats KB", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
  });

  it("formats MB", () => {
    expect(formatBytes(1024 * 1024)).toBe("1.0 MB");
  });

  it("formats GB", () => {
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1.0 GB");
  });

  it("formats TB", () => {
    expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
  });
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
