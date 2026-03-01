import { useState, useEffect, useCallback } from "preact/hooks";

/**
 * Generic polling hook: calls `fetcher` immediately and every `intervalMs`.
 * Returns [data, loading, refetch, error].
 */
export function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs = 5000,
): [T | null, boolean, () => void, Error | null] {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  const doFetch = useCallback(() => {
    fetcher()
      .then((result) => {
        setData(result);
        setError(null);
        setLoading(false);
      })
      .catch((err) => {
        setError(err instanceof Error ? err : new Error(String(err)));
        setLoading(false);
      });
  }, [fetcher]);

  useEffect(() => {
    doFetch();
    const id = setInterval(doFetch, intervalMs);
    return () => clearInterval(id);
  }, [doFetch, intervalMs]);

  return [data, loading, doFetch, error];
}
