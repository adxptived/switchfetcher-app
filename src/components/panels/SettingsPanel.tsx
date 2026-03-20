import { Toggle } from "../Toggle";
import { useEffect, useState } from "react";
import { formatRelativeTime } from "../../utils/date";
import type { AppSettings, CachePrefs, NotificationPermissionState } from "../../types";

type UpdateStatus = "checking" | "up_to_date" | "update_available" | "error";

interface SettingsPanelProps {
  isOpen: boolean;
  settingsDraft: AppSettings | null;
  cacheInfo: { lastUpdated: Date | null; accountCount: number };
  cachePrefs: CachePrefs;
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
  onClearCache: () => void;
  onCachePrefsChange: (prefs: CachePrefs) => void;
  onRequestNotificationPermission: () => void;
  onSendTestNotification: () => void;
  onOpenGitHub: () => void;
  onCheckForUpdates: () => void;
}

export function SettingsPanel({
  isOpen,
  settingsDraft,
  cacheInfo,
  cachePrefs,
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
  onClearCache,
  onCachePrefsChange,
  onRequestNotificationPermission,
  onSendTestNotification,
  onOpenGitHub,
  onCheckForUpdates,
}: SettingsPanelProps) {
  const sectionTitleClass = "flex items-center gap-2.5 text-sm font-semibold";
  const [, setTick] = useState(0);

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setTick((value) => value + 1);
    }, 30000);

    return () => window.clearInterval(intervalId);
  }, []);

  const permissionTone =
    notificationPermission === "granted"
      ? "#16a34a"
      : notificationPermission === "denied"
        ? "#dc2626"
        : "#d97706";
  const cacheSummary = cacheInfo.lastUpdated
    ? `Last updated: ${formatRelativeTime(cacheInfo.lastUpdated)} · ${cacheInfo.accountCount} account${cacheInfo.accountCount === 1 ? "" : "s"}`
    : "No cache";

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
              <div>
                <div className={sectionTitleClass} style={{ color: "var(--color-text-primary)" }}>
                  <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M4 4v6h6" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M20 20v-6h-6" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M20 9a8 8 0 00-13.66-4.66L4 6" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M4 15a8 8 0 0013.66 4.66L20 18" />
                  </svg>
                  Background refresh
                </div>
                <div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>One backend scheduler updates usage cards and tray state.</div>
              </div>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <span>Enable background refresh</span>
                <Toggle checked={settingsDraft.background_refresh_enabled} onChange={(checked) => onFieldChange("background_refresh_enabled", checked)} />
              </label>
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
              <div>
                <div className={sectionTitleClass} style={{ color: "var(--color-text-primary)" }}>
                  <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M4.5 7.5A2.5 2.5 0 017 5h10a2.5 2.5 0 012.5 2.5v9A2.5 2.5 0 0117 19H7a2.5 2.5 0 01-2.5-2.5v-9z" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M8.5 9.5h7M8.5 13h5" />
                  </svg>
                  Cache
                </div>
                <div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Usage data is cached in localStorage for instant startup.</div>
              </div>
              <div className="rounded-lg border p-3 text-sm" style={{ background: "var(--color-bg-muted)", borderColor: "var(--color-border)", color: "var(--color-text-secondary)" }}>
                {cacheSummary}
              </div>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <span>Enable cache on startup</span>
                <Toggle checked={cachePrefs.enabled} onChange={(checked) => onCachePrefsChange({ ...cachePrefs, enabled: checked })} />
              </label>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <span>Cache TTL (minutes)</span>
                <input
                  type="number"
                  min="0"
                  value={cachePrefs.ttlMinutes}
                  onChange={(event) => onCachePrefsChange({ ...cachePrefs, ttlMinutes: Number(event.target.value) })}
                  className="h-10 w-32 rounded-lg px-3 text-sm sf-input"
                />
              </label>
              <div className="text-xs" style={{ color: "var(--color-text-muted)" }}>Set to 0 to never expire cached usage data.</div>
              <div className="flex flex-wrap gap-2">
                <button onClick={onClearCache} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Clear Cache</button>
              </div>
            </div>
            <div className="rounded-xl p-4 space-y-4 sf-panel">
              <div>
                <div className={sectionTitleClass} style={{ color: "var(--color-text-primary)" }}>
                  <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9" />
                  </svg>
                  Notifications
                </div>
                <div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Desktop notifications for reset recovery, threshold alerts and quick smoke-testing.</div>
              </div>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <span>Enable notifications</span>
                <Toggle checked={settingsDraft.notifications_enabled} onChange={(checked) => onFieldChange("notifications_enabled", checked)} />
              </label>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <span>Claude reset notifications</span>
                <Toggle checked={settingsDraft.claude_reset_notifications_enabled} onChange={(checked) => onFieldChange("claude_reset_notifications_enabled", checked)} />
              </label>
              <label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <span>24-hour clock</span>
                <Toggle checked={settingsDraft.use_24h_time} onChange={(checked) => onFieldChange("use_24h_time", checked)} />
              </label>
              <div className="space-y-2 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                <div className="flex items-center justify-between gap-3">
                  <span>Usage alert threshold</span>
                  <span>{settingsDraft.usage_alert_threshold ? `${settingsDraft.usage_alert_threshold}%` : "Off"}</span>
                </div>
                <input type="range" min="50" max="95" step="5" value={settingsDraft.usage_alert_threshold ?? 80} onChange={(event) => onFieldChange("usage_alert_threshold", Number(event.target.value) as AppSettings["usage_alert_threshold"])} disabled={settingsDraft.usage_alert_threshold == null} className="w-full" />
                <label className="flex items-center gap-2">
                  <Toggle checked={settingsDraft.usage_alert_threshold != null} onChange={(checked) => onFieldChange("usage_alert_threshold", checked ? 80 : null)} />
                  Enable usage threshold alerts
                </label>
              </div>
              <div className="rounded-lg border p-3 text-sm" style={{ background: "var(--color-bg-muted)", borderColor: "var(--color-border)", color: "var(--color-text-secondary)" }}>
                <span className="inline-flex items-center gap-2">
                  <span className="h-2.5 w-2.5 rounded-full" style={{ background: permissionTone }} />
                  Permission state: <span className="font-medium">{notificationPermission}</span>
                </span>
              </div>
              <div className="flex flex-wrap gap-2"><button onClick={onRequestNotificationPermission} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Request Permission</button><button onClick={onSendTestNotification} disabled={sendingTestNotification} className="rounded-lg px-3 py-2 text-sm sf-btn-primary disabled:opacity-50">{sendingTestNotification ? "Sending..." : "Send Test Notification"}</button></div>
            </div>
            <div className="rounded-xl p-4 space-y-4 sf-panel">
              <div>
                <div className={sectionTitleClass} style={{ color: "var(--color-text-primary)" }}>
                  <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <circle cx="12" cy="12" r="8.5" strokeWidth={1.8} />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.8} d="M12 10v5" />
                    <circle cx="12" cy="7.5" r="1" fill="currentColor" stroke="none" />
                  </svg>
                  About / Updates
                </div>
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
