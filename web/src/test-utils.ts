import { vi } from "vitest";

/**
 * Mock fetch with URL-substring-based routing.
 * Each key in `routes` is matched against the URL; first match wins.
 * Unmatched URLs return an empty JSON array.
 */
export function mockFetch(routes: Record<string, unknown>) {
  vi.spyOn(globalThis, "fetch").mockImplementation((url) => {
    const path = typeof url === "string" ? url : (url as Request).url;
    for (const [substring, body] of Object.entries(routes)) {
      if (path.includes(substring)) {
        return Promise.resolve(new Response(JSON.stringify(body)));
      }
    }
    return Promise.resolve(new Response(JSON.stringify([])));
  });
}

/**
 * Mock fetch that never resolves (simulates pending/loading state).
 */
export function mockFetchPending() {
  vi.spyOn(globalThis, "fetch").mockReturnValue(new Promise(() => {}));
}
