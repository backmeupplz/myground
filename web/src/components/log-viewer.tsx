import { useState, useEffect, useRef } from "preact/hooks";

const MAX_LINES = 200;

interface Props {
  serviceId: string;
}

export function LogViewer({ serviceId }: Props) {
  const [lines, setLines] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let ws: WebSocket | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let stopped = false;

    const connect = () => {
      if (stopped) return;
      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${proto}//${window.location.host}/api/services/${serviceId}/logs`;
      ws = new WebSocket(url);

      ws.onopen = () => setConnected(true);
      ws.onclose = () => {
        setConnected(false);
        if (!stopped) reconnectTimer = setTimeout(connect, 3000);
      };
      ws.onerror = () => setConnected(false);

      ws.onmessage = (event) => {
        setLines((prev) => {
          const next = [...prev, event.data as string];
          return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
        });
      };
    };

    connect();

    return () => {
      stopped = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      ws?.close();
    };
  }, [serviceId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  return (
    <div class="bg-gray-950 rounded-lg border border-gray-800 overflow-hidden">
      <div class="flex items-center justify-between px-3 py-2 border-b border-gray-800">
        <span class="text-xs text-gray-500 font-mono">Logs</span>
        <span
          class={`text-xs ${connected ? "text-green-400" : "text-gray-500"}`}
        >
          {connected ? "Connected" : "Disconnected"}
        </span>
      </div>
      <div class="h-64 overflow-y-auto p-3 font-mono text-xs text-green-400 leading-relaxed">
        {lines.length === 0 && (
          <span class="text-gray-600">Waiting for logs...</span>
        )}
        {lines.map((line, i) => (
          <div key={i}>{line}</div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
