import { route } from "preact-router";

interface Props {
  currentPath: string;
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

export function Sidebar({ currentPath }: Props) {
  return (
    <div class="w-14 bg-gray-900 border-r border-gray-800 flex flex-col items-center py-4 gap-2 shrink-0">
      {/* Services / Dashboard */}
      <NavButton
        active={currentPath === "/" || currentPath.startsWith("/service/")}
        onClick={() => route("/")}
        title="Services"
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
        >
          <path d="M19.35 10.04A7.49 7.49 0 0 0 12 4a7.48 7.48 0 0 0-6.93 4.64A5.5 5.5 0 0 0 6.5 20h12.25a4.25 4.25 0 0 0 .6-8.46z" />
        </svg>
      </NavButton>

      <div class="flex-1" />

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
