export function TailscaleGuide() {
  return (
    <div class="space-y-3">
      {/* Step-by-step key generation */}
      <div>
        <p class="text-sm font-medium text-gray-300 mb-1">
          How to get an auth key:
        </p>
        <ol class="text-sm text-gray-400 list-decimal list-inside space-y-0.5">
          <li>
            Sign up or log in at{" "}
            <a
              href="https://login.tailscale.com"
              target="_blank"
              rel="noopener noreferrer"
              class="text-amber-400 hover:text-amber-300 underline"
            >
              login.tailscale.com
            </a>
          </li>
          <li>
            Go to{" "}
            <a
              href="https://login.tailscale.com/admin/settings/keys"
              target="_blank"
              rel="noopener noreferrer"
              class="text-amber-400 hover:text-amber-300 underline"
            >
              Settings &gt; Keys
            </a>
          </li>
          <li>Click "Generate auth key"</li>
          <li>Enable "Reusable", then paste the key below</li>
        </ol>
      </div>

      {/* Device install info */}
      <div class="flex gap-2 bg-gray-800/50 rounded p-3">
        <span class="text-blue-400 shrink-0" aria-hidden="true">
          &#9432;
        </span>
        <p class="text-sm text-gray-400">
          Install the{" "}
          <a
            href="https://tailscale.com/download"
            target="_blank"
            rel="noopener noreferrer"
            class="text-amber-400 hover:text-amber-300 underline"
          >
            Tailscale app
          </a>{" "}
          on every device you want to connect (phone, laptop, tablet). All
          devices must use the same Tailscale account.
        </p>
      </div>

      {/* Exit node + Pi-hole tip */}
      <div class="flex gap-2 bg-gray-800/50 rounded p-3">
        <span class="text-amber-400 shrink-0" aria-hidden="true">
          &#9733;
        </span>
        <p class="text-sm text-gray-400">
          Set MyGround as an exit node to get Pi-hole ad blocking on all your
          devices, even when you're away from home.
        </p>
      </div>
    </div>
  );
}
