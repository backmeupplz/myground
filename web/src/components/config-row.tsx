import { useState } from "preact/hooks";

interface Props {
  label: string;
  value: string;
  isPassword: boolean;
}

export function ConfigRow({ label, value, isPassword }: Props) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(value).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <div class="bg-gray-900 rounded-lg px-4 py-3 flex items-center justify-between">
      <div class="min-w-0 mr-3">
        <span class="text-gray-200">{label}</span>
        <p class="text-xs text-gray-500 font-mono truncate">
          {isPassword ? "\u2022".repeat(8) : value}
        </p>
      </div>
      <button
        class="text-xs text-blue-400 hover:text-blue-300 shrink-0"
        onClick={handleCopy}
      >
        {copied ? "Copied!" : "Copy"}
      </button>
    </div>
  );
}
