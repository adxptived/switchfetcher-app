import type { DiagnosticsSnapshot } from "../../types";

interface DiagnosticsPanelProps {
  isOpen: boolean;
  diagnosticsLoading: boolean;
  diagnostics: DiagnosticsSnapshot | null;
  onClose: () => void;
  onRefreshDiagnostics: () => void;
  formatHistoryDate: (value: string) => string;
}

function CopyIcon() {
  return (
    <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <rect x="9" y="9" width="10" height="10" rx="2" strokeWidth={1.8} />
      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M15 9V7a2 2 0 00-2-2H7a2 2 0 00-2 2v6a2 2 0 002 2h2" />
    </svg>
  );
}

function getProviderAccent(provider: DiagnosticsSnapshot["providers"][number]["provider"]) {
  if (provider === "codex") return "border-l-4 border-emerald-500";
  if (provider === "claude") return "border-l-4 border-orange-500";
  return "border-l-4 border-blue-500";
}

export function DiagnosticsPanel({
  isOpen,
  diagnosticsLoading,
  diagnostics,
  onClose,
  onRefreshDiagnostics,
  formatHistoryDate,
}: DiagnosticsPanelProps) {
  if (!isOpen) return null;

  const handleCopy = async (value: string) => {
    await navigator.clipboard.writeText(value);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay">
      <div className="mx-4 w-full max-w-3xl rounded-2xl sf-panel">
        <div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}>
          <h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Safe Diagnostics</h2>
          <div className="flex items-center gap-2">
            <button onClick={onRefreshDiagnostics} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Refresh</button>
            <button onClick={onClose} style={{ color: "var(--color-text-muted)" }}>✕</button>
          </div>
        </div>
        <div className="max-h-[70vh] space-y-5 overflow-y-auto p-5">
          {diagnosticsLoading || !diagnostics ? (
            <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Loading diagnostics...</p>
          ) : (
            <>
              <div className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}>
                <div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>App version</div>
                <div className="font-semibold" style={{ color: "var(--color-text-primary)" }}>{diagnostics.app_version}</div>
              </div>
              <div className="grid gap-3 md:grid-cols-3">
                {diagnostics.providers.map((provider) => (
                  <div
                    key={provider.provider}
                    className={`rounded-xl border p-4 ${getProviderAccent(provider.provider)}`}
                  >
                    <div className="text-sm font-semibold capitalize" style={{ color: "var(--color-text-primary)" }}>
                      {provider.provider}
                    </div>
                    <div className="mt-2 space-y-1 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                      <div>Supports switch: {provider.supports_switch ? "yes" : "no"}</div>
                      <div>Active: {provider.active_account_name ?? "none"}</div>
                      <div className="flex items-start gap-2">
                        <span className="break-all">Path: {provider.credential_path ?? "n/a"}</span>
                        {provider.credential_path ? (
                          <button
                            onClick={() => void handleCopy(provider.credential_path!)}
                            className="rounded-md p-1 sf-btn-secondary"
                            title="Copy credential path"
                            aria-label={`Copy ${provider.provider} credential path`}
                          >
                            <CopyIcon />
                          </button>
                        ) : null}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
              <div className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}>
                <div className="mb-3 text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Broken accounts</div>
                <div className="space-y-3">
                  {diagnostics.broken_accounts.length === 0 ? (
                    <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>No broken accounts.</p>
                  ) : (
                    diagnostics.broken_accounts.map((broken) => (
                      <div key={broken.account_id} className="rounded-lg border border-amber-100 bg-amber-50 p-3">
                        <div className="font-medium text-amber-900">
                          {broken.name} <span className="text-xs uppercase text-amber-700">{broken.provider}</span>
                        </div>
                        <div className="text-sm text-amber-800">{broken.reason}</div>
                        <div className="mt-1 text-xs text-amber-700">{broken.suggested_source ?? "Manual re-import required"}</div>
                      </div>
                    ))
                  )}
                </div>
              </div>
              <div className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}>
                <div className="mb-3 text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Recent errors</div>
                <div className="space-y-3">
                  {diagnostics.recent_errors.length === 0 ? (
                    <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>No recent errors.</p>
                  ) : (
                    diagnostics.recent_errors.map((errorEntry, index) => (
                      <div key={`${errorEntry.created_at}-${index}`} className="rounded-lg border border-red-100 bg-red-50 p-3">
                        <div className="font-medium text-red-900">{errorEntry.summary}</div>
                        <div className="text-sm text-red-700">{errorEntry.detail ?? errorEntry.kind}</div>
                        <div className="mt-1 text-xs text-red-500">{formatHistoryDate(errorEntry.created_at)}</div>
                      </div>
                    ))
                  )}
                </div>
              </div>
            </>
          )}
        </div>
        <div className="flex gap-3 border-t p-5" style={{ borderColor: "var(--color-border)" }}>
          <button
            onClick={() => void (diagnostics ? handleCopy(JSON.stringify(diagnostics, null, 2)) : Promise.resolve())}
            disabled={!diagnostics}
            className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-secondary disabled:opacity-50"
          >
            Copy all as JSON
          </button>
          <button onClick={onClose} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary">Close</button>
        </div>
      </div>
    </div>
  );
}
