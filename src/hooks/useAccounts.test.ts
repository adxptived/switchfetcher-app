import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, ProviderCapabilities, UsageInfo } from "../types";

const { listeners, listenMock } = vi.hoisted(() => {
  const hoistedListeners = new Map<string, (event: { payload: unknown }) => void>();
  const hoistedListenMock = vi.fn(
    async <T,>(eventName: string, callback: (event: { payload: T }) => void) => {
      hoistedListeners.set(eventName, callback as (event: { payload: unknown }) => void);
      return () => {
        hoistedListeners.delete(eventName);
      };
    },
  );

  return {
    listeners: hoistedListeners,
    listenMock: hoistedListenMock,
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

const ipcMocks = vi.hoisted(() => ({
  addAccountFromFile: vi.fn(),
  addSessionCookieAccount: vi.fn(),
  cancelLogin: vi.fn(),
  completeLogin: vi.fn(),
  deleteAccount: vi.fn(),
  deleteAccountsBulk: vi.fn(),
  exportAccountsFullEncryptedFile: vi.fn(),
  exportAccountsSlimText: vi.fn(),
  exportSelectedAccountsFullEncryptedFile: vi.fn(),
  exportSelectedAccountsSlimText: vi.fn(),
  getAppSettings: vi.fn(),
  getBestAccountRecommendation: vi.fn(),
  getDiagnostics: vi.fn(),
  getNotificationPermissionState: vi.fn(),
  getProviderCapabilities: vi.fn(),
  getUsage: vi.fn(),
  importAccountsFullEncryptedFile: vi.fn(),
  importAccountsSlimText: vi.fn(),
  importClaudeCredentials: vi.fn(),
  importClaudeCredentialsFromPath: vi.fn(),
  importGeminiCredentials: vi.fn(),
  importGeminiCredentialsFromPath: vi.fn(),
  listAccountHistory: vi.fn(),
  listAccounts: vi.fn(),
  refreshAllAccountsUsage: vi.fn(),
  refreshSelectedAccountsUsage: vi.fn(),
  renameAccount: vi.fn(),
  repairAccountSecret: vi.fn(),
  requestNotificationPermission: vi.fn(),
  sendTestNotification: vi.fn(),
  setAccountTags: vi.fn(),
  setProviderHidden: vi.fn(),
  startLogin: vi.fn(),
  switchAccount: vi.fn(),
  updateAppSettings: vi.fn(),
  warmupAccount: vi.fn(),
  warmupAllAccounts: vi.fn(),
}));

vi.mock("../ipc", () => ipcMocks);

import { useAccounts } from "./useAccounts";

const DEFAULT_SETTINGS: AppSettings = {
  background_refresh_enabled: true,
  base_refresh_interval_seconds: 60,
  notifications_enabled: true,
  claude_reset_notifications_enabled: true,
  use_24h_time: false,
  usage_alert_threshold: 80,
};

const DEFAULT_CAPABILITIES: ProviderCapabilities = {
  provider: "codex",
  supports_switch: true,
  supports_usage: true,
  supports_warmup: true,
  supports_import_export: true,
  supports_background_watch: false,
  credential_path: null,
};

const ACCOUNT = {
  id: "account-1",
  name: "Codex Account",
  provider: "codex" as const,
  tags: [],
  hidden: false,
  email: "account@example.com",
  plan_type: "team",
  auth_mode: "chat_gpt" as const,
  capabilities: DEFAULT_CAPABILITIES,
  last_action: null,
  last_refresh_error: null,
  load_state: "ready" as const,
  unavailable_reason: null,
  repair_hint: null,
  is_active: false,
  created_at: "2025-01-01T00:00:00Z",
  last_used_at: null,
};

const UPDATED_USAGE: UsageInfo = {
  account_id: ACCOUNT.id,
  plan_type: "team",
  primary_used_percent: 12,
  primary_window_minutes: 60,
  primary_resets_at: 1_763_000_000,
  secondary_used_percent: null,
  secondary_window_minutes: null,
  secondary_resets_at: null,
  has_credits: null,
  unlimited_credits: null,
  credits_balance: null,
  quota_status: "healthy",
  daily_stats: null,
  skipped: false,
  error: null,
};

describe("useAccounts", () => {
  beforeEach(() => {
    listeners.clear();
    listenMock.mockClear();
    localStorage.clear();

    Object.values(ipcMocks).forEach((mock) => mock.mockReset());
    ipcMocks.listAccounts.mockResolvedValue([ACCOUNT]);
    ipcMocks.refreshAllAccountsUsage.mockResolvedValue([]);
    ipcMocks.refreshSelectedAccountsUsage.mockResolvedValue([]);
    ipcMocks.getUsage.mockResolvedValue(UPDATED_USAGE);
    ipcMocks.getAppSettings.mockResolvedValue(DEFAULT_SETTINGS);
    ipcMocks.updateAppSettings.mockResolvedValue(DEFAULT_SETTINGS);
    ipcMocks.getNotificationPermissionState.mockResolvedValue("default");
    ipcMocks.requestNotificationPermission.mockResolvedValue("default");
    ipcMocks.getProviderCapabilities.mockResolvedValue([]);
    ipcMocks.getBestAccountRecommendation.mockResolvedValue(null);
    ipcMocks.getDiagnostics.mockResolvedValue(null);
    ipcMocks.listAccountHistory.mockResolvedValue([]);
    ipcMocks.warmupAllAccounts.mockResolvedValue({
      total_accounts: 0,
      warmed_accounts: 0,
      failed_account_ids: [],
    });
    ipcMocks.completeLogin.mockResolvedValue(ACCOUNT);
  });

  it("changing cache prefs does not re-run bootstrap or re-register listeners", async () => {
    const { result } = renderHook(() => useAccounts());

    await waitFor(() => {
      expect(ipcMocks.listAccounts).toHaveBeenCalledTimes(1);
      expect(listenMock).toHaveBeenCalledTimes(3);
    });

    await act(async () => {
      result.current.setCachePrefs({ enabled: false, ttlMinutes: 0 });
    });

    expect(ipcMocks.listAccounts).toHaveBeenCalledTimes(1);
    expect(listenMock).toHaveBeenCalledTimes(3);
  });

  it("persists usage-updated events to localStorage when cache is enabled", async () => {
    renderHook(() => useAccounts());

    await waitFor(() => {
      expect(listeners.has("usage-updated")).toBe(true);
    });

    const usageUpdated = listeners.get("usage-updated");
    expect(usageUpdated).toBeDefined();

    await act(async () => {
      usageUpdated?.({ payload: UPDATED_USAGE });
    });

    const raw = localStorage.getItem("sf-usage-cache");
    expect(raw).not.toBeNull();
    expect(raw).toContain("\"account_id\":\"account-1\"");
  });

  it("refreshSingleUsage uses current cache prefs instead of a stale closure", async () => {
    const { result } = renderHook(() => useAccounts());

    await waitFor(() => {
      expect(ipcMocks.listAccounts).toHaveBeenCalled();
    });

    await act(async () => {
      result.current.setCachePrefs({ enabled: false, ttlMinutes: 0 });
    });
    localStorage.removeItem("sf-usage-cache");

    await act(async () => {
      await result.current.refreshSingleUsage(ACCOUNT.id);
    });

    expect(localStorage.getItem("sf-usage-cache")).toBeNull();
  });
});
