import type { AccountAction } from "../../types";

interface HistoryPanelProps {
  isOpen: boolean;
  historyLoading: boolean;
  historyEntries: AccountAction[];
  onClose: () => void;
  formatHistoryDate: (value: string) => string;
}

export function HistoryPanel({
  isOpen,
  historyLoading,
  historyEntries,
  onClose,
  formatHistoryDate,
}: HistoryPanelProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay">
      <div className="mx-4 w-full max-w-3xl rounded-2xl sf-panel">
        <div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Action History</h2><button onClick={onClose} style={{ color: "var(--color-text-muted)" }}>✕</button></div>
        <div className="max-h-[70vh] space-y-3 overflow-y-auto p-5">{historyLoading ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Loading history...</p> : historyEntries.length === 0 ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>No history yet.</p> : historyEntries.map((entry) => <div key={entry.id} className="rounded-xl border p-3" style={{ borderColor: "var(--color-border)" }}><div className="flex items-center justify-between gap-3"><div className="font-medium" style={{ color: "var(--color-text-primary)" }}>{entry.summary}</div><div className="text-xs" style={{ color: "var(--color-text-muted)" }}>{formatHistoryDate(entry.created_at)}</div></div><div className="mt-1 text-sm" style={{ color: "var(--color-text-secondary)" }}>{entry.provider ? `${entry.provider} • ` : ""}{entry.detail ?? entry.kind}</div></div>)}</div>
      </div>
    </div>
  );
}
