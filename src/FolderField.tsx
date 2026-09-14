import { useId, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { selectedFolderValue } from "./folderPath";

export function FolderField({
  label,
  value,
  onChange,
  placeholder,
  dialogTitle,
  disabled = false,
  createChildName,
  buttonLabel = "Browse…",
  hint,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  dialogTitle: string;
  disabled?: boolean;
  createChildName?: string;
  buttonLabel?: string;
  hint?: string;
}) {
  const inputId = useId();
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function browse() {
    setPicking(true);
    setError(null);
    try {
      const selected = await open({
        title: dialogTitle,
        directory: true,
        multiple: false,
        recursive: true,
        defaultPath: value.trim() || undefined,
      });
      if (typeof selected === "string") {
        onChange(selectedFolderValue(selected, createChildName));
      }
    } catch (requestError) {
      setError(requestError instanceof Error ? requestError.message : String(requestError));
    } finally {
      setPicking(false);
    }
  }

  return (
    <div className="folder-field">
      <label htmlFor={inputId}>{label}</label>
      <div className="folder-field-row">
        <input
          disabled={disabled || picking}
          id={inputId}
          onChange={(event) => onChange(event.target.value)}
          placeholder={placeholder}
          value={value}
        />
        <button
          className="outline-button"
          disabled={disabled || picking}
          onClick={() => void browse()}
          type="button"
        >
          {picking ? "Opening…" : buttonLabel}
        </button>
      </div>
      {hint && <p className="field-help folder-field-help">{hint}</p>}
      {error && <p className="field-error" role="alert">{error}</p>}
    </div>
  );
}
