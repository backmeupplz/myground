import { route } from "preact-router";

interface Props {
  currentPath: string;
  updateAvailable?: boolean;
}

function NavButton({
  active,
  onClick,
  title,
  children,
}: {
  active: boolean;
  onClick: () => void;
  title: string;
  children: preact.ComponentChildren;
}) {
  return (
    <button
      class={`w-10 h-10 flex items-center justify-center rounded-lg transition-colors ${
        active
          ? "bg-gray-700 text-white"
          : "text-gray-500 hover:text-gray-300 hover:bg-gray-800"
      }`}
      onClick={onClick}
      title={title}
    >
      {children}
    </button>
  );
}

export function Sidebar({ currentPath, updateAvailable }: Props) {
  return (
    <div class="w-14 bg-gray-900 border-r border-gray-800 flex flex-col items-center py-4 gap-2 shrink-0">
      {/* Apps / Dashboard */}
      <NavButton
        active={currentPath === "/" || currentPath.startsWith("/app/")}
        onClick={() => route("/")}
        title="Apps"
      >
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <rect x="3" y="3" width="7" height="7" rx="1" />
          <rect x="14" y="3" width="7" height="7" rx="1" />
          <rect x="3" y="14" width="7" height="7" rx="1" />
          <rect x="14" y="14" width="7" height="7" rx="1" />
        </svg>
      </NavButton>

      {/* Backups */}
      <NavButton
        active={currentPath === "/backups"}
        onClick={() => route("/backups")}
        title="Backups"
      >
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <path d="M12 16V8m0 0l-4 4m4-4l4 4" />
          <path d="M20 21H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2z" />
        </svg>
      </NavButton>

      {/* Tailscale */}
      <NavButton
        active={currentPath === "/tailscale"}
        onClick={() => route("/tailscale")}
        title="Tailscale"
      >
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <circle cx="12" cy="12" r="10" />
          <path d="M2 12h20" />
          <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
        </svg>
      </NavButton>

      {/* Cloudflare */}
      <NavButton
        active={currentPath === "/cloudflare"}
        onClick={() => route("/cloudflare")}
        title="Cloudflare"
      >
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
          overflow="visible"
        >
          <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
        </svg>
      </NavButton>

      <div class="flex-1" />

      {/* Updates */}
      <NavButton
        active={currentPath === "/updates"}
        onClick={() => route("/updates")}
        title="Updates"
      >
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <path d="M16.023 9.348h4.992v-.001M2.985 19.644v-4.992m0 0h4.992m-4.993 0 3.181 3.183a8.25 8.25 0 0 0 13.803-3.7M4.031 9.865a8.25 8.25 0 0 1 13.803-3.7l3.181 3.182m0-4.991v4.99" />
        </svg>
      </NavButton>

      {/* Settings */}
      <NavButton
        active={currentPath === "/settings"}
        onClick={() => route("/settings")}
        title="Settings"
      >
        <svg
          class="w-5 h-5"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          viewBox="0 0 24 24"
        >
          <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      </NavButton>
    </div>
  );
}
