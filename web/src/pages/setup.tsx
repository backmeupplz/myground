import { useState } from "preact/hooks";
import { api } from "../api";
import { TailscaleGuide } from "../components/tailscale-guide";

interface Props {
  onComplete: () => void;
}

export function Setup({ onComplete }: Props) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [tailscaleKey, setTailscaleKey] = useState("");
  const [cloudflareToken, setCloudflareToken] = useState("");
  const [cloudflareStatus, setCloudflareStatus] = useState<
    "" | "connecting" | "connected" | "error"
  >("");
  const [cloudflareError, setCloudflareError] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError("");

    if (!username.trim() || !password) {
      setError("Username and password are required.");
      return;
    }
    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }
    if (password.length < 8) {
      setError("Password must be at least 8 characters.");
      return;
    }

    setLoading(true);
    try {
      await api.setup({
        username: username.trim(),
        password,
        tailscale_key: tailscaleKey.trim() || undefined,
      });
      onComplete();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Setup failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="min-h-screen bg-gray-950 flex items-center justify-center p-6">
      <div class="w-full max-w-md">
        <h1 class="text-3xl font-bold text-gray-100 mb-2">MyGround Setup</h1>
        <p class="text-gray-400 mb-8">
          Create your admin account to get started.
        </p>

        <form onSubmit={handleSubmit} class="space-y-5">
          {/* Account */}
          <div>
            <label class="block text-sm font-medium text-gray-300 mb-1">
              Username
            </label>
            <input
              type="text"
              value={username}
              onInput={(e) =>
                setUsername((e.target as HTMLInputElement).value)
              }
              class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
              placeholder="admin"
              autoFocus
            />
          </div>

          <div>
            <label class="block text-sm font-medium text-gray-300 mb-1">
              Password
            </label>
            <input
              type="password"
              value={password}
              onInput={(e) =>
                setPassword((e.target as HTMLInputElement).value)
              }
              class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
            />
          </div>

          <div>
            <label class="block text-sm font-medium text-gray-300 mb-1">
              Confirm Password
            </label>
            <input
              type="password"
              value={confirmPassword}
              onInput={(e) =>
                setConfirmPassword((e.target as HTMLInputElement).value)
              }
              class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500"
            />
          </div>

          {/* Tailscale (optional) */}
          <div class="border-t border-gray-800 pt-5">
            <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
              Tailscale (optional)
            </h2>
            <p class="text-sm text-gray-500 mb-3">
              Enable remote access to your services via Tailscale. Each service
              gets its own HTTPS domain on your tailnet. The key is used once to
              register and is not stored.
            </p>
            <TailscaleGuide />
            <input
              type="text"
              value={tailscaleKey}
              onInput={(e) =>
                setTailscaleKey((e.target as HTMLInputElement).value)
              }
              class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500 font-mono text-sm"
              placeholder="tskey-auth-..."
            />
          </div>

          {/* Cloudflare (optional) */}
          <div class="border-t border-gray-800 pt-5">
            <h2 class="text-sm font-medium text-gray-400 mb-3 uppercase tracking-wider">
              Cloudflare (optional)
            </h2>
            <p class="text-sm text-gray-500 mb-3">
              Expose services on custom domains like photos.yourdomain.com via
              Cloudflare Tunnels. You can set this up later in Settings.
            </p>
            <div class="text-sm text-gray-400 space-y-2 mb-3">
              <p>
                Create a Cloudflare API token at{" "}
                <a
                  href="https://dash.cloudflare.com/profile/api-tokens"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="text-amber-400 hover:text-amber-300 underline"
                >
                  dash.cloudflare.com/profile/api-tokens
                </a>{" "}
                with permissions:
              </p>
              <ul class="list-disc list-inside text-gray-500 space-y-1">
                <li>Account &gt; Cloudflare Tunnel &gt; Edit</li>
                <li>Zone &gt; DNS &gt; Edit</li>
                <li>Account Settings &gt; Read</li>
              </ul>
            </div>
            {cloudflareStatus === "connected" ? (
              <p class="text-green-400 text-sm">
                Cloudflare connected successfully.
              </p>
            ) : (
              <div class="flex gap-2">
                <input
                  type="password"
                  value={cloudflareToken}
                  onInput={(e) =>
                    setCloudflareToken(
                      (e.target as HTMLInputElement).value,
                    )
                  }
                  class="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 focus:outline-none focus:border-gray-500 font-mono text-sm"
                  placeholder="Cloudflare API token"
                />
                <button
                  type="button"
                  disabled={
                    cloudflareStatus === "connecting" ||
                    !cloudflareToken.trim()
                  }
                  onClick={async () => {
                    setCloudflareStatus("connecting");
                    setCloudflareError("");
                    try {
                      await api.saveCloudflareConfig({
                        enabled: true,
                        api_token: cloudflareToken.trim(),
                      });
                      setCloudflareStatus("connected");
                    } catch (err: unknown) {
                      setCloudflareStatus("error");
                      setCloudflareError(
                        err instanceof Error
                          ? err.message
                          : "Connection failed",
                      );
                    }
                  }}
                  class="px-4 py-2 bg-amber-600 hover:bg-amber-500 text-white text-sm rounded disabled:opacity-50"
                >
                  {cloudflareStatus === "connecting"
                    ? "Connecting..."
                    : "Connect"}
                </button>
              </div>
            )}
            {cloudflareError && (
              <p class="text-red-400 text-sm mt-2">{cloudflareError}</p>
            )}
          </div>

          {error && (
            <p class="text-red-400 text-sm">{error}</p>
          )}

          <button
            type="submit"
            disabled={loading}
            class="w-full py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
          >
            {loading ? "Setting up..." : "Complete Setup"}
          </button>
        </form>
      </div>
    </div>
  );
}
