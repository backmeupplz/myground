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
              gets its own HTTPS domain on your tailnet.
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
