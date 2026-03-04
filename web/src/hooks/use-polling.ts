import { useState, useEffect, useCallback, useRef } from "preact/hooks";

/**
 * Generic polling hook: calls `fetcher` immediately and every `intervalMs`.
 * `intervalMs` can be a number or a function returning a number (for adaptive polling).
 * Returns [data, loading, refetch, error].
 */
export function usePolling<T>(
  fetcher: () => Promise<T>,
  intervalMs: number | (() => number) = 5000,
): [T | null, boolean, () => void, Error | null] {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const getInterval = useCallback(
    () => (typeof intervalMs === "function" ? intervalMs() : intervalMs),
    [intervalMs],
  );

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

    const schedule = () => {
      timerRef.current = setTimeout(() => {
        doFetch();
        schedule();
      }, getInterval());
    };
    schedule();

    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [doFetch, getInterval]);

  return [data, loading, doFetch, error];
}
