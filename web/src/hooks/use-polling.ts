import { useState, useEffect, useCallback } from "preact/hooks";

/**
 * Generic polling hook: calls `fetcher` immediately and every `intervalMs`.
 * Returns [data, loading, refetch].
 */
export function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs = 5000,
): [T | null, boolean, () => void] {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);

  const doFetch = useCallback(() => {
    fetcher()
      .then((result) => {
        setData(result);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [fetcher]);

  useEffect(() => {
    doFetch();
    const id = setInterval(doFetch, intervalMs);
    return () => clearInterval(id);
  }, [doFetch, intervalMs]);

  return [data, loading, doFetch];
}
