import type { AppSettings, NotificationPermissionState } from "../../types";

type UpdateStatus = "checking" | "up_to_date" | "update_available" | "error";

interface SettingsPanelProps {
  isOpen: boolean;
  settingsDraft: AppSettings | null;
  notificationPermission: NotificationPermissionState;
  settingsSaving: boolean;
  sendingTestNotification: boolean;
  refreshIntervalError: string | null;
  appVersion: string;
  updateStatus: UpdateStatus | null;
  latestVersion: string | null;
  updateMessage: string | null;
  onClose: () => void;
  onFieldChange: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
  onSave: () => void;
  onRequestNotificationPermission: () => void;
  onSendTestNotification: () => void;
  onOpenGitHub: () => void;
  onCheckForUpdates: () => void;
}

export function SettingsPanel({
  isOpen,
  settingsDraft,
  notificationPermission,
  settingsSaving,
  sendingTestNotification,
  refreshIntervalError,
  appVersion,
  updateStatus,
  latestVersion,
  updateMessage,
  onClose,
  onFieldChange,
  onSave,
  onRequestNotificationPermission,
  onSendTestNotification,
  onOpenGitHub,
  onCheckForUpdates,
}: SettingsPanelProps) {
  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay">
      <div className="mx-4 w-full max-w-2xl rounded-2xl sf-panel">
        <div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}>
          <h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Settings</h2>
          <button onClick={onClose} style={{ color: "var(--color-text-muted)" }}>✕</button>
        </div>
        <div className="max-h-[70vh] space-y-5 overflow-y-auto p-5">
          {!settingsDraft ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Loading settings...</p> : <>
            <div className="rounded-xl p-4 space-y-4 sf-panel">
              <div><div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Background refresh</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>One backend scheduler updates usage cards and tray state.</div></div>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Enable background refresh</span><input type="checkbox" checked={settingsDraft.background_refresh_enabled} onChange={(event) => onFieldChange("background_refresh_enabled", event.target.checked)} /></label>
              <div className="space-y-2">
                <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                  <span>Refresh interval</span>
                  <input type="number" value={settingsDraft.base_refresh_interval_seconds} onChange={(event) => onFieldChange("base_refresh_interval_seconds", Number(event.target.value) as AppSettings["base_refresh_interval_seconds"])} className={`h-10 w-32 rounded-lg px-3 text-sm sf-input ${refreshIntervalError ? "border-red-300 bg-red-50 text-red-600" : ""}`} />
                </label>
                <div className="text-xs" style={{ color: "var(--color-text-muted)" }}>Allowed values: 60, 90, 120, 300 seconds.</div>
                {refreshIntervalError ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-600">{refreshIntervalError}</div> : null}
              </div>
            </div>
            <div className="rounded-xl p-4 space-y-4 sf-panel">
              <div><div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Notifications</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Desktop notifications for reset recovery, threshold alerts and quick smoke-testing.</div></div>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Enable notifications</span><input type="checkbox" checked={settingsDraft.notifications_enabled} onChange={(event) => onFieldChange("notifications_enabled", event.target.checked)} /></label>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Claude reset notifications</span><input type="checkbox" checked={settingsDraft.claude_reset_notifications_enabled} onChange={(event) => onFieldChange("claude_reset_notifications_enabled", event.target.checked)} /></label>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>24-hour clock</span><input type="checkbox" checked={settingsDraft.use_24h_time} onChange={(event) => onFieldChange("use_24h_time", event.target.checked)} /></label>
              <div className="space-y-2 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <div className="flex items-center justify-between gap-3">
                  <span>Usage alert threshold</span>
                  <span>{settingsDraft.usage_alert_threshold ? `${settingsDraft.usage_alert_threshold}%` : "Off"}</span>
                </div>
                <input type="range" min="50" max="95" step="5" value={settingsDraft.usage_alert_threshold ?? 80} onChange={(event) => onFieldChange("usage_alert_threshold", Number(event.target.value) as AppSettings["usage_alert_threshold"])} disabled={settingsDraft.usage_alert_threshold == null} className="w-full" />
                <label className="flex items-center gap-2">
                  <input type="checkbox" checked={settingsDraft.usage_alert_threshold != null} onChange={(event) => onFieldChange("usage_alert_threshold", event.target.checked ? 80 : null)} />
                  Enable usage threshold alerts
                </label>
              </div>
              <div className="rounded-lg border p-3 text-sm" style={{ background: "var(--color-bg-muted)", borderColor: "var(--color-border)", color: "var(--color-text-secondary)" }}>Permission state: <span className="font-medium">{notificationPermission}</span></div>
              <div className="flex flex-wrap gap-2"><button onClick={onRequestNotificationPermission} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Request Permission</button><button onClick={onSendTestNotification} disabled={sendingTestNotification} className="rounded-lg px-3 py-2 text-sm sf-btn-primary disabled:opacity-50">{sendingTestNotification ? "Sending..." : "Send Test Notification"}</button></div>
            </div>
            <div className="rounded-xl p-4 space-y-4 sf-panel">
              <div>
                <div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>About / Updates</div>
                <div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Open the project repository and compare your build against the latest GitHub release.</div>
              </div>
              <div className="rounded-lg border p-3 text-sm" style={{ background: "var(--color-bg-muted)", borderColor: "var(--color-border)", color: "var(--color-text-secondary)" }}>
                Installed version: <span className="font-medium" style={{ color: "var(--color-text-primary)" }}>{appVersion ? `v${appVersion}` : "Loading..."}</span>
                {latestVersion ? <span> · Latest release: <span className="font-medium" style={{ color: "var(--color-text-primary)" }}>{latestVersion}</span></span> : null}
              </div>
              <div className="flex flex-wrap gap-2">
                <button onClick={onOpenGitHub} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Open GitHub Repository</button>
                <button onClick={onCheckForUpdates} disabled={updateStatus === "checking"} className="rounded-lg px-3 py-2 text-sm sf-btn-primary disabled:opacity-50">{updateStatus === "checking" ? "Checking..." : "Check for Updates"}</button>
              </div>
              {updateMessage ? (
                <div
                  className="rounded-lg border p-3 text-sm"
                  style={{
                    borderColor:
                      updateStatus === "error"
                        ? "rgb(254 202 202)"
                        : updateStatus === "update_available"
                          ? "rgb(191 219 254)"
                          : "var(--color-border)",
                    background:
                      updateStatus === "error"
                        ? "rgb(254 242 242)"
                        : updateStatus === "update_available"
                          ? "rgb(239 246 255)"
                          : "var(--color-bg-muted)",
                    color:
                      updateStatus === "error"
                        ? "rgb(185 28 28)"
                        : updateStatus === "update_available"
                          ? "rgb(30 64 175)"
                          : "var(--color-text-secondary)",
                  }}
                >
                  {updateMessage}
                </div>
              ) : null}
            </div>
          </>}
        </div>
        <div className="flex gap-3 border-t p-5" style={{ borderColor: "var(--color-border)" }}><button onClick={onClose} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-secondary">Close</button><button onClick={onSave} disabled={!settingsDraft || settingsSaving || Boolean(refreshIntervalError)} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary disabled:opacity-50">{settingsSaving ? "Saving..." : "Save Settings"}</button></div>
      </div>
    </div>
  );
}
