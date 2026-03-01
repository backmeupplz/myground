import { useState } from "preact/hooks";
import { type InstallVariable } from "../api";
import { PathPicker } from "./path-picker";

interface Props {
  variable: InstallVariable;
  value: string;
  onChange: (key: string, value: string) => void;
}

export function VariableField({ variable, value, onChange }: Props) {
  const [editingPath, setEditingPath] = useState(false);

  if (variable.input_type === "path") {
    if (editingPath) {
      return (
        <div>
          <FieldLabel variable={variable} />
          <PathPicker
            initialPath={value || "/"}
            onSelect={(path) => {
              onChange(variable.key, path);
              setEditingPath(false);
            }}
            onCancel={() => setEditingPath(false)}
          />
        </div>
      );
    }

    return (
      <div>
        <FieldLabel variable={variable} />
        <div class="flex items-center gap-2">
          <span class="flex-1 bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono truncate">
            {value || variable.default || "/"}
          </span>
          <button
            class="px-3 py-1.5 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded shrink-0"
            onClick={() => setEditingPath(true)}
          >
            Browse
          </button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <FieldLabel variable={variable} />
      <input
        type={variable.input_type === "password" ? "password" : variable.input_type === "email" ? "email" : "text"}
        value={value}
        onInput={(e) =>
          onChange(variable.key, (e.target as HTMLInputElement).value)
        }
        class="w-full bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm text-gray-200 font-mono"
        placeholder={variable.default ?? ""}
      />
    </div>
  );
}

function FieldLabel({ variable }: { variable: InstallVariable }) {
  return (
    <label class="text-xs text-gray-500 block mb-1">
      {variable.label}
      {variable.required && <span class="text-red-400 ml-1">*</span>}
    </label>
  );
}
