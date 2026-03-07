import { useState } from "preact/hooks";

interface Props {
  label: string;
  value: string;
  isPassword: boolean;
}

function fallbackCopy(text: string, onSuccess: () => void) {
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  document.execCommand("copy");
  document.body.removeChild(ta);
  onSuccess();
}

export function ConfigRow({ label, value, isPassword }: Props) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    const onSuccess = () => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    };
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(value).then(onSuccess).catch(() => {
        fallbackCopy(value, onSuccess);
      });
    } else {
      fallbackCopy(value, onSuccess);
    }
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
