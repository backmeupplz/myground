import { useState } from "preact/hooks";
import { api } from "../api";

interface Props {
  onLogin: () => void;
}

export function Login({ onLogin }: Props) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      await api.login(username, password);
      onLogin();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Login failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div class="min-h-screen bg-gray-950 flex items-center justify-center p-6">
      <div class="w-full max-w-sm">
        <h1 class="text-3xl font-bold text-gray-100 mb-2">MyGround</h1>
        <p class="text-gray-400 mb-8">Sign in to continue.</p>

        <form onSubmit={handleSubmit} class="space-y-4">
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

          {error && <p class="text-red-400 text-sm">{error}</p>}

          <button
            type="submit"
            disabled={loading}
            class="w-full py-2 bg-amber-600 hover:bg-amber-500 text-white font-medium rounded disabled:opacity-50"
          >
            {loading ? "Signing in..." : "Sign In"}
          </button>
        </form>
      </div>
    </div>
  );
}
