interface ExportImportModalProps {
  isOpen: boolean;
  mode: "slim_export" | "slim_import";
  payload: string;
  copied: boolean;
  error: string | null;
  isImporting: boolean;
  onClose: () => void;
  onPayloadChange: (value: string) => void;
  onCopy: () => void;
  onImport: () => void;
}

export function ExportImportModal({
  isOpen,
  mode,
  payload,
  copied,
  error,
  isImporting,
  onClose,
  onPayloadChange,
  onCopy,
  onImport,
}: ExportImportModalProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay">
      <div className="mx-4 w-full max-w-2xl rounded-2xl sf-panel">
        <div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}>
          <h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>{mode === "slim_export" ? "Export Slim Text" : "Import Slim Text"}</h2>
          <button onClick={onClose} style={{ color: "var(--color-text-muted)" }}>✕</button>
        </div>
        <div className="space-y-4 p-5">
          <textarea value={payload} onChange={(event) => onPayloadChange(event.target.value)} readOnly={mode === "slim_export"} placeholder={mode === "slim_export" ? "Export string will appear here" : "Paste config string here"} className="h-48 w-full rounded-lg px-4 py-3 text-sm font-mono sf-input" />
          {error ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-600">{error}</div> : null}
        </div>
        <div className="flex gap-3 border-t p-5" style={{ borderColor: "var(--color-border)" }}>
          <button onClick={onClose} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-secondary">Close</button>
          {mode === "slim_export" ? (
            <button onClick={onCopy} disabled={!payload} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary disabled:opacity-50">{copied ? "Copied" : "Copy String"}</button>
          ) : (
            <button onClick={onImport} disabled={isImporting} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary disabled:opacity-50">{isImporting ? "Importing..." : "Import Missing Accounts"}</button>
          )}
        </div>
      </div>
    </div>
  );
}
