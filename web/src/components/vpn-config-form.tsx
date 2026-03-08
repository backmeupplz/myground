interface VpnConfigFormProps {
  vpnProvider: string;
  vpnType: string;
  vpnCountry: string;
  vpnPortForward: boolean;
  vpnEnvVars: Record<string, string>;
  vpnSaving: boolean;
  vpnError: string | null;
  onProviderChange: (provider: string) => void;
  onTypeChange: (type: string) => void;
  onCountryChange: (country: string) => void;
  onPortForwardChange: (enabled: boolean) => void;
  onEnvVarsChange: (vars: Record<string, string>) => void;
  onSave: () => void;
  onCancel?: () => void;
  saveLabel?: string;
  savingLabel?: string;
  /** Show the redacted credentials warning (settings page) */
  vpnHasRedacted?: boolean;
  /** Extra note appended to port forwarding description (e.g. qbittorrent hint) */
  portForwardNote?: string;
  /** Show "Saved" feedback */
  vpnSaved?: boolean;
  /** Test connection handler — if provided, renders "Test Connection" button */
  onTest?: () => void;
  vpnTesting?: boolean;
  vpnTestResult?: boolean | null;
  vpnTestLogs?: string[];
}

export function VpnConfigForm({
  vpnProvider,
  vpnType,
  vpnCountry,
  vpnPortForward,
  vpnEnvVars,
  vpnSaving,
  vpnError,
  onProviderChange,
  onTypeChange,
  onCountryChange,
  onPortForwardChange,
  onEnvVarsChange,
  onSave,
  onCancel,
  saveLabel = "Save",
  savingLabel = "Saving...",
  vpnHasRedacted,
  portForwardNote,
  vpnSaved,
  onTest,
  vpnTesting,
  vpnTestResult,
  vpnTestLogs,
}: VpnConfigFormProps) {
  const countryLinks: Record<string, string> = {
    protonvpn: "https://protonvpn.com/vpn-servers",
    nordvpn: "https://nordvpn.com/servers/",
    mullvad: "https://mullvad.net/en/servers",
    custom: "https://github.com/qdm12/gluetun-wiki/tree/main/setup/providers",
  };

  return (
    <div class="space-y-3">
      <div>
        <label class="block text-xs text-gray-400 mb-1">Provider</label>
        <select
          value={vpnProvider}
          onChange={(e) => {
            onProviderChange((e.target as HTMLSelectElement).value);
            onEnvVarsChange({});
          }}
          class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
        >
          <option value="protonvpn">ProtonVPN</option>
          <option value="nordvpn">NordVPN</option>
          <option value="mullvad">Mullvad</option>
          <option value="custom">Custom</option>
        </select>
      </div>
      <div>
        <label class="block text-xs text-gray-400 mb-1">VPN Type</label>
        <select
          value={vpnType}
          onChange={(e) => onTypeChange((e.target as HTMLSelectElement).value)}
          class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
        >
          <option value="openvpn">OpenVPN</option>
          <option value="wireguard">WireGuard</option>
        </select>
      </div>
      {vpnType === "openvpn" && (
        <>
          <div>
            <label class="block text-xs text-gray-400 mb-1">Username</label>
            <input
              type="text"
              value={vpnEnvVars["OPENVPN_USER"] || ""}
              onInput={(e) => onEnvVarsChange({ ...vpnEnvVars, OPENVPN_USER: (e.target as HTMLInputElement).value })}
              class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
            />
          </div>
          <div>
            <label class="block text-xs text-gray-400 mb-1">Password</label>
            <input
              type="password"
              value={vpnEnvVars["OPENVPN_PASSWORD"] || ""}
              onInput={(e) => onEnvVarsChange({ ...vpnEnvVars, OPENVPN_PASSWORD: (e.target as HTMLInputElement).value })}
              class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
            />
          </div>
          {vpnProvider === "protonvpn" && (
            <p class="text-xs text-gray-500">
              Use your <a href="https://account.protonvpn.com/account#openvpn" target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline">OpenVPN/IKEv2 credentials</a>, not your Proton account password. Required if you have 2FA enabled.
              {vpnPortForward && " Append +pmp to your username (e.g. user123+pmp) for port forwarding to work."}
            </p>
          )}
        </>
      )}
      {vpnType === "wireguard" && (
        <div>
          <label class="block text-xs text-gray-400 mb-1">Private Key</label>
          <input
            type="password"
            value={vpnEnvVars["WIREGUARD_PRIVATE_KEY"] || ""}
            onInput={(e) => onEnvVarsChange({ ...vpnEnvVars, WIREGUARD_PRIVATE_KEY: (e.target as HTMLInputElement).value })}
            class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
          />
        </div>
      )}
      <div>
        <label class="block text-xs text-gray-400 mb-1">
          Server Country (optional) — <a href={countryLinks[vpnProvider] || countryLinks.custom} target="_blank" rel="noopener noreferrer" class="text-amber-400 hover:text-amber-300 underline">see supported countries</a>
        </label>
        <input
          type="text"
          value={vpnCountry}
          onInput={(e) => onCountryChange((e.target as HTMLInputElement).value)}
          placeholder="e.g. Netherlands"
          class="w-full px-3 py-2 bg-gray-800 border border-gray-700 rounded text-gray-100 text-sm focus:outline-none focus:border-gray-500"
        />
      </div>
      <div>
        <label class="flex items-center gap-2 text-sm text-gray-300">
          <input
            type="checkbox"
            checked={vpnPortForward}
            onChange={(e) => onPortForwardChange((e.target as HTMLInputElement).checked)}
            class="rounded bg-gray-800 border-gray-600"
          />
          Enable port forwarding (recommended)
        </label>
        <p class="text-xs text-gray-500 mt-1">
          Required for torrent seeding and other apps that need to accept incoming connections.
          Leave this on unless you know you don't need it.{portForwardNote}
        </p>
      </div>
      {vpnHasRedacted && (
        <p class="text-xs text-amber-400">
          Credentials are redacted. Re-enter them to update.
        </p>
      )}
      <div class="flex items-center gap-3 flex-wrap">
        <button
          disabled={vpnSaving}
          class="px-3 py-1.5 bg-green-600 hover:bg-green-500 text-white text-xs rounded disabled:opacity-50"
          onClick={onSave}
        >
          {vpnSaving ? savingLabel : saveLabel}
        </button>
        {onTest && (
          <button
            class="px-4 py-2 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded disabled:opacity-50"
            disabled={vpnTesting || vpnSaving}
            onClick={onTest}
          >
            {vpnTesting ? "Testing..." : "Test Connection"}
          </button>
        )}
        {onCancel && (
          <button
            class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-xs rounded"
            onClick={onCancel}
          >
            Cancel
          </button>
        )}
        {vpnSaved && <span class="text-green-400 text-sm">Saved</span>}
        {vpnError && <span class="text-red-400 text-sm">{vpnError}</span>}
        {vpnTestResult !== undefined && vpnTestResult !== null && (
          <span class={`text-sm ${vpnTestResult ? "text-green-400" : "text-red-400"}`}>
            {vpnTestResult ? "Connected" : "Failed"}
          </span>
        )}
      </div>
      {vpnTestLogs && vpnTestLogs.length > 0 && (
        <pre class="mt-2 p-3 bg-gray-950 rounded text-xs text-gray-400 font-mono max-h-48 overflow-y-auto whitespace-pre-wrap">
          {vpnTestLogs.join("\n")}
        </pre>
      )}
    </div>
  );
}
