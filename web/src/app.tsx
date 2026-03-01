import { useState, useEffect } from "preact/hooks";
import Router from "preact-router";
import { api, type HealthResponse } from "./api";
import { Dashboard } from "./pages/dashboard";
import { ServiceDetail } from "./pages/service-detail";
import { Settings } from "./pages/settings";
import { Backups } from "./pages/backups";
import { Sidebar } from "./components/sidebar";

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [currentPath, setCurrentPath] = useState(window.location.pathname);

  useEffect(() => {
    api.health().then(setHealth).catch(() => setHealth(null));
  }, []);

  return (
    <div class="min-h-screen bg-gray-950 text-gray-100 flex">
      {health && <Sidebar currentPath={currentPath} />}

      <div class="flex-1 flex flex-col min-w-0">
        <header class="px-6 py-4 flex items-center gap-3 border-b border-gray-800">
          <a href="/" class="text-xl font-bold hover:text-gray-300">
            MyGround
          </a>
          {health && (
            <span class="text-xs text-gray-500">v{health.version}</span>
          )}
        </header>

        {health ? (
          <Router onChange={(e) => setCurrentPath(e.url)}>
            <Dashboard path="/" />
            <ServiceDetail path="/service/:id" />
            <Backups path="/backups" />
            <Settings path="/settings" />
          </Router>
        ) : (
          <div class="flex-1 flex items-center justify-center">
            <p class="text-gray-500">Connecting...</p>
          </div>
        )}
      </div>
    </div>
  );
}
