import { useState, useEffect } from "preact/hooks";
import { ServiceList } from "./services";
import { DiskList } from "./disks";

interface HealthResponse {
  status: string;
  version: string;
}

type Tab = "services" | "disks";

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [tab, setTab] = useState<Tab>("services");

  useEffect(() => {
    fetch("/api/health")
      .then((r) => r.json())
      .then(setHealth)
      .catch(() => setHealth(null));
  }, []);

  return (
    <div class="min-h-screen bg-gray-950 text-gray-100 flex flex-col items-center px-4 py-12">
      <div class="text-center mb-8">
        <h1 class="text-4xl font-bold mb-2">MyGround</h1>
        <p class="text-gray-400 mb-4">Hold your ground.</p>
        {health ? (
          <p class="text-green-400 text-sm">
            Server: {health.status} &middot; v{health.version}
          </p>
        ) : (
          <p class="text-gray-500">Connecting...</p>
        )}
      </div>

      {health && (
        <>
          <div class="flex gap-4 mb-6">
            <button
              class={`px-4 py-2 rounded text-sm font-medium ${
                tab === "services"
                  ? "bg-gray-700 text-white"
                  : "bg-gray-900 text-gray-400 hover:text-gray-200"
              }`}
              onClick={() => setTab("services")}
            >
              Services
            </button>
            <button
              class={`px-4 py-2 rounded text-sm font-medium ${
                tab === "disks"
                  ? "bg-gray-700 text-white"
                  : "bg-gray-900 text-gray-400 hover:text-gray-200"
              }`}
              onClick={() => setTab("disks")}
            >
              Disks
            </button>
          </div>

          {tab === "services" && <ServiceList />}
          {tab === "disks" && <DiskList />}
        </>
      )}
    </div>
  );
}
