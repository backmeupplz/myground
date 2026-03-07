import { describe, it, expect, afterEach, vi } from "vitest";
import {
  api,
  generatePassword,
  containerColor,
  containerIcon,
  formatBytes,
  formatTimestamp,
  linkify,
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

describe("formatTimestamp", () => {
  it("formats valid ISO string", () => {
    const result = formatTimestamp("2024-01-15T10:30:00Z");
    // Should return a locale string, not the original ISO
    expect(result).not.toBe("2024-01-15T10:30:00Z");
    expect(result.length).toBeGreaterThan(0);
  });

  it("returns original string for invalid date", () => {
    expect(formatTimestamp("not-a-date")).toBe("not-a-date");
    expect(formatTimestamp("")).toBe("");
  });
});

describe("linkify", () => {
  it("wraps URLs in anchor tags", () => {
    const result = linkify("Visit https://example.com for info");
    expect(result).toContain('href="https://example.com"');
    expect(result).toContain("target=\"_blank\"");
  });

  it("escapes HTML entities", () => {
    const result = linkify("<script>alert('xss')</script>");
    expect(result).toContain("&lt;script&gt;");
    expect(result).not.toContain("<script>");
  });

  it("handles text with no URLs", () => {
    const result = linkify("plain text here");
    expect(result).toBe("plain text here");
  });

  it("escapes quotes", () => {
    const result = linkify('a "quoted" value');
    expect(result).toContain("&quot;quoted&quot;");
  });

  it("escapes ampersands", () => {
    const result = linkify("foo & bar");
    expect(result).toContain("foo &amp; bar");
  });

  it("handles multiple URLs", () => {
    const result = linkify("go to https://a.com and https://b.com");
    expect(result).toContain('href="https://a.com"');
    expect(result).toContain('href="https://b.com"');
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

  it("api.apps calls correct URL", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify([])),
    );

    await api.apps();
    expect(fetchSpy).toHaveBeenCalledWith("/api/apps", undefined);
  });

  it("api.installApp calls with POST and JSON body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({ ok: true, message: "installed", port: 9000 }),
      ),
    );

    await api.installApp("whoami", { storage_path: "/mnt/data" });
    const [url, opts] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/apps/whoami/install");
    expect(opts.method).toBe("POST");
    expect(JSON.parse(opts.body as string)).toEqual({
      storage_path: "/mnt/data",
    });
  });

  it("api.startApp calls correct URL with POST", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ ok: true, message: "started" })),
    );

    await api.startApp("whoami");
    const [url, opts] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/apps/whoami/start");
    expect(opts.method).toBe("POST");
  });

  it("api.removeApp calls correct URL with DELETE", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ ok: true, message: "removed" })),
    );

    await api.removeApp("whoami");
    const [url, opts] = fetchSpy.mock.calls[0];
    expect(url).toBe("/api/apps/whoami");
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
