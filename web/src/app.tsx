import { useState, useEffect } from "preact/hooks";
import Router from "preact-router";
import { api, setOnUnauthorized, type HealthResponse } from "./api";
import { Dashboard } from "./pages/dashboard";
import { ServiceDetail } from "./pages/service-detail";
import { Settings } from "./pages/settings";
import { Backups } from "./pages/backups";
import { Tailscale } from "./pages/tailscale";
import { Cloudflare } from "./pages/cloudflare";
import { Setup } from "./pages/setup";
import { Login } from "./pages/login";
import { Sidebar } from "./components/sidebar";

type AuthState = "loading" | "setup" | "login" | "authenticated";

export function App() {
  const [authState, setAuthState] = useState<AuthState>("loading");
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [currentPath, setCurrentPath] = useState(window.location.pathname);
  const [ipCopied, setIpCopied] = useState(false);

  const checkAuth = () => {
    api
      .authStatus()
      .then((status) => {
        if (status.setup_required) {
          setAuthState("setup");
        } else if (status.authenticated) {
          setAuthState("authenticated");
        } else {
          setAuthState("login");
        }
      })
      .catch(() => setAuthState("login"));
  };

  useEffect(() => {
    // Set up 401 handler
    setOnUnauthorized(() => setAuthState("login"));
    checkAuth();
  }, []);

  useEffect(() => {
    if (authState === "authenticated") {
      api.health().then(setHealth).catch(() => setHealth(null));
    }
  }, [authState]);

  if (authState === "loading") {
    return (
      <div class="min-h-screen bg-gray-950 flex items-center justify-center">
        <p class="text-gray-500">Connecting...</p>
      </div>
    );
  }

  if (authState === "setup") {
    return <Setup onComplete={checkAuth} />;
  }

  if (authState === "login") {
    return <Login onLogin={checkAuth} />;
  }

  const handleLogout = async () => {
    await api.logout().catch(() => {});
    setAuthState("login");
    setHealth(null);
  };

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
          <div class="flex-1" />
          {health?.server_ip && (
            <button
              class="text-xs text-gray-500 hover:text-gray-300 font-mono cursor-pointer"
              title="Click to copy IP"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(health.server_ip!);
                  setIpCopied(true);
                  setTimeout(() => setIpCopied(false), 2000);
                } catch {}
              }}
            >
              {ipCopied ? "Copied!" : health.server_ip}
            </button>
          )}
        </header>

        {health ? (
          <Router onChange={(e) => setCurrentPath(e.url)}>
            <Dashboard path="/" />
            <ServiceDetail path="/service/:id" />
            <Backups path="/backups" />
            <Tailscale path="/tailscale" />
            <Cloudflare path="/cloudflare" />
            <Settings path="/settings" onLogout={handleLogout} />
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
