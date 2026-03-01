interface Props {
  label: string;
  type: string;
  value: string;
  placeholder?: string;
  onInput: (value: string) => void;
}

export function Field({ label, type, value, placeholder, onInput }: Props) {
  return (
    <div>
      <label class="text-xs text-gray-500 block mb-1">{label}</label>
      <input
        type={type}
        value={value}
        onInput={(e) => onInput((e.target as HTMLInputElement).value)}
        class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200"
        placeholder={placeholder}
      />
    </div>
  );
}
