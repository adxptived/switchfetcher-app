export { default } from "./AppShell";
/*
import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useAccounts } from "./hooks/useAccounts";
import { AccountCard, AddAccountModal } from "./components";
import type {
  AccountAction,
  AccountWithUsage,
  AppSettings,
  CodexProcessInfo,
  DiagnosticsSnapshot,
  Provider,
} from "./types";
import "./App.css";

type ConfigModalMode = "slim_export" | "slim_import";
type SortMode = "deadline_asc" | "deadline_desc" | "remaining_desc" | "remaining_asc";
const PROVIDERS: Provider[] = ["codex", "claude", "gemini"];
const REFRESH_INTERVAL_OPTIONS: AppSettings["base_refresh_interval_seconds"][] = [
  60,
  90,
  120,
  300,
];
const FILTER_OPTIONS = ["all", "codex", "claude", "gemini"] as const;

function MoonIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={1.8}
        d="M21 12.8A9 9 0 1111.2 3a7 7 0 009.8 9.8z"
      />
    </svg>
  );
}

function SunIcon() {
  return (
    <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <circle cx="12" cy="12" r="4" strokeWidth={1.8} />
      <path strokeLinecap="round" strokeWidth={1.8} d="M12 2.5v2.2M12 19.3v2.2M21.5 12h-2.2M4.7 12H2.5M18.7 5.3l-1.6 1.6M6.9 17.1l-1.6 1.6M18.7 18.7l-1.6-1.6M6.9 6.9L5.3 5.3" />
    </svg>
  );
}

function formatError(err: unknown) {
  if (!err) return "Unknown error";
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string") return err;
  try { return JSON.stringify(err); } catch { return "Unknown error"; }
}
function formatHistoryDate(value: string) { return new Date(value).toLocaleString(); }
function formatResetAt(value: number | null | undefined) {
  if (!value) return "No reset window";
  return new Date(value * 1000).toLocaleString();
}
function getRemainingPercent(account: AccountWithUsage) {
  if (account.usage?.primary_used_percent == null) return -1;
  return Math.max(0, 100 - account.usage.primary_used_percent);
}
function formatPlanLabel(planType: string | null | undefined, authMode: AccountWithUsage["auth_mode"]) {
  if (!planType) {
    return authMode === "api_key" ? "API Key" : "Unknown";
  }
  if (planType === planType.toUpperCase()) {
    return planType;
  }
  return planType.charAt(0).toUpperCase() + planType.slice(1);
}
function computeLoadedBestAccount(accounts: AccountWithUsage[], provider: Provider) {
  return accounts
    .filter((account) => account.provider === provider && account.load_state === "ready" && account.capabilities.supports_switch && !account.hidden && !account.usage?.error)
    .sort((left, right) => {
      const remainingDiff = getRemainingPercent(right) - getRemainingPercent(left);
      if (remainingDiff !== 0) return remainingDiff;
      const leftReset = left.usage?.primary_resets_at ?? Number.MAX_SAFE_INTEGER;
      const rightReset = right.usage?.primary_resets_at ?? Number.MAX_SAFE_INTEGER;
      if (leftReset !== rightReset) return leftReset - rightReset;
      return left.name.localeCompare(right.name);
    })[0];
}

function App() {
  const {
    accounts, appSettings, notificationPermission, loading, error, refreshUsage, refreshSingleUsage, refreshSelectedUsage,
    warmupAccount, warmupAllAccounts, switchAccount, deleteAccount, renameAccount,
    setAccountTags, setProviderHidden, listAccountHistory, getBestAccountRecommendation,
    getDiagnostics, importFromFile, importClaudeCredentials, importClaudeCredentialsFromPath,
    importGeminiCredentials, importGeminiCredentialsFromPath, addGeminiAccount,
    repairAccountSecret,
    exportAccountsSlimText, exportSelectedAccountsSlimText, importAccountsSlimText,
    exportAccountsFullEncryptedFile, exportSelectedAccountsFullEncryptedFile,
    importAccountsFullEncryptedFile, loadAppSettings, updateAppSettings,
    requestNotificationPermission, sendTestNotification,
    startOAuthLogin, completeOAuthLogin, cancelOAuthLogin,
  } = useAccounts();

  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isConfigModalOpen, setIsConfigModalOpen] = useState(false);
  const [configModalMode, setConfigModalMode] = useState<ConfigModalMode>("slim_export");
  const [configPayload, setConfigPayload] = useState("");
  const [configModalError, setConfigModalError] = useState<string | null>(null);
  const [configCopied, setConfigCopied] = useState(false);
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [processInfo, setProcessInfo] = useState<CodexProcessInfo | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isExportingSlim, setIsExportingSlim] = useState(false);
  const [isImportingSlim, setIsImportingSlim] = useState(false);
  const [isExportingFull, setIsExportingFull] = useState(false);
  const [isImportingFull, setIsImportingFull] = useState(false);
  const [isWarmingAll, setIsWarmingAll] = useState(false);
  const [warmingUpId, setWarmingUpId] = useState<string | null>(null);
  const [refreshSuccess, setRefreshSuccess] = useState(false);
  const [toast, setToast] = useState<{ message: string; isError: boolean } | null>(null);
  const [maskedAccounts, setMaskedAccounts] = useState<Set<string>>(new Set());
  const [selectedAccountIds, setSelectedAccountIds] = useState<Set<string>>(new Set());
  const [otherAccountsSort, setOtherAccountsSort] = useState<SortMode>("deadline_asc");
  const [filterProvider, setFilterProvider] = useState<Provider | "all">("all");
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    const stored = localStorage.getItem("sf-theme");
    return stored === "dark" || stored === "light" ? stored : "light";
  });
  const [tagFilter, setTagFilter] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [isActionsMenuOpen, setIsActionsMenuOpen] = useState(false);
  const [isHistoryOpen, setIsHistoryOpen] = useState(false);
  const [isDiagnosticsOpen, setIsDiagnosticsOpen] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isRecommendationPickerOpen, setIsRecommendationPickerOpen] = useState(false);
  const [historyEntries, setHistoryEntries] = useState<AccountAction[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSnapshot | null>(null);
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<AppSettings | null>(null);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [sendingTestNotification, setSendingTestNotification] = useState(false);
  const actionsMenuRef = useRef<HTMLDivElement | null>(null);

  const showToast = useCallback((message: string, isError = false) => {
    setToast({ message, isError });
    window.setTimeout(() => setToast(null), 2600);
  }, []);
  const toggleMask = useCallback((accountId: string) => {
    setMaskedAccounts((prev) => {
      const next = new Set(prev);
      if (next.has(accountId)) next.delete(accountId); else next.add(accountId);
      return next;
    });
  }, []);
  const toggleSelect = useCallback((accountId: string) => {
    setSelectedAccountIds((prev) => {
      const next = new Set(prev);
      if (next.has(accountId)) next.delete(accountId); else next.add(accountId);
      return next;
    });
  }, []);

  const checkProcesses = useCallback(async () => {
    try {
      const info = await invoke<CodexProcessInfo>("check_codex_processes");
      setProcessInfo(info);
      return info;
    } catch (err) {
      console.error("Failed to check processes:", err);
      return null;
    }
  }, []);

  useEffect(() => {
    void checkProcesses();
    const interval = setInterval(() => void checkProcesses(), 3000);
    return () => clearInterval(interval);
  }, [checkProcesses]);

  useEffect(() => {
    if (!isActionsMenuOpen) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (actionsMenuRef.current?.contains(event.target as Node)) return;
      setIsActionsMenuOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [isActionsMenuOpen]);
  useEffect(() => {
    if (appSettings) {
      setSettingsDraft(appSettings);
    }
  }, [appSettings]);
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("sf-theme", theme);
  }, [theme]);
  const filteredAccounts = useMemo(() => {
    const normalizedTag = tagFilter.trim().toLowerCase();
    return accounts.filter((account) => {
      if (!showHidden && account.hidden) return false;
      if (filterProvider !== "all" && account.provider !== filterProvider) return false;
      if (!normalizedTag) return true;
      return account.tags.some((tag) => tag.toLowerCase().includes(normalizedTag));
    });
  }, [accounts, filterProvider, showHidden, tagFilter]);

  const activeAccounts = accounts.filter((account) => account.is_active);
  const otherAccounts = filteredAccounts.filter((account) => !account.is_active);
  const hasRunningProcesses = Boolean(processInfo?.count);
  const selectedIds = useMemo(() => [...selectedAccountIds], [selectedAccountIds]);
  const selectedAccounts = useMemo(() => filteredAccounts.filter((account) => selectedAccountIds.has(account.id)), [filteredAccounts, selectedAccountIds]);
  const failedAccountIds = useMemo(() => filteredAccounts.filter((account) => account.usage?.error).map((account) => account.id), [filteredAccounts]);
  const availableTags = useMemo(() => Array.from(new Set(accounts.flatMap((account) => account.tags))).sort(), [accounts]);
  const loadedBestByProvider = useMemo(() => PROVIDERS.map((provider) => ({ provider, account: computeLoadedBestAccount(accounts, provider) })), [accounts]);
  const bestByProvider = useMemo(
    () =>
      new Map(
        loadedBestByProvider.map(({ provider, account }) => [provider, account] as const)
      ),
    [loadedBestByProvider]
  );

  const sortedOtherAccounts = useMemo(() => {
    const getReset = (account: AccountWithUsage) => account.usage?.primary_resets_at ?? Number.POSITIVE_INFINITY;
    return [...otherAccounts].sort((left, right) => {
      if (otherAccountsSort === "deadline_asc") return getReset(left) - getReset(right);
      if (otherAccountsSort === "deadline_desc") return getReset(right) - getReset(left);
      if (otherAccountsSort === "remaining_desc") return getRemainingPercent(right) - getRemainingPercent(left);
      return getRemainingPercent(left) - getRemainingPercent(right);
    });
  }, [otherAccounts, otherAccountsSort]);

  const getSwitchDisabledReason = useCallback((account: AccountWithUsage) => {
    if (account.load_state !== "ready") return account.unavailable_reason ?? "Account needs repair";
    if (!account.capabilities.supports_switch) return `${account.provider} accounts are usage-only right now`;
    if (account.provider === "codex" && hasRunningProcesses) return "Close all Codex processes first";
    return null;
  }, [hasRunningProcesses]);

  const handleRepair = useCallback(async (account: AccountWithUsage) => {
    try {
      await repairAccountSecret(account.id);
      showToast(`Repair finished for ${account.name}`);
    } catch (err) {
      showToast(`Repair failed for ${account.name}: ${formatError(err)}`, true);
    }
  }, [repairAccountSecret, showToast]);

  const handleSwitch = useCallback(async (accountId: string) => {
    const account = accounts.find((entry) => entry.id === accountId);
    if (!account) return;
    if (account.provider === "codex") {
      const info = await checkProcesses();
      if (info && !info.can_switch) return;
    }
    try {
      setSwitchingId(accountId);
      await switchAccount(accountId);
    } catch (err) {
      showToast(`Switch failed: ${formatError(err)}`, true);
    } finally {
      setSwitchingId(null);
    }
  }, [accounts, checkProcesses, showToast, switchAccount]);

  const handleSwitchBest = useCallback(async (provider: Provider) => {
    setIsRecommendationPickerOpen(false);
    try {
      const recommendation = await getBestAccountRecommendation(provider);
      if (!recommendation) {
        showToast(`No switchable ${provider} account is ready right now`, true);
        return;
      }
      await handleSwitch(recommendation.account_id);
    } catch (err) {
      showToast(`Best-account switch failed: ${formatError(err)}`, true);
    }
  }, [getBestAccountRecommendation, handleSwitch, showToast]);

  const handleDelete = async (accountId: string) => {
    if (deleteConfirmId !== accountId) {
      setDeleteConfirmId(accountId);
      window.setTimeout(() => setDeleteConfirmId(null), 3000);
      return;
    }
    try {
      await deleteAccount(accountId);
      setDeleteConfirmId(null);
      setSelectedAccountIds((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    } catch (err) {
      showToast(`Delete failed: ${formatError(err)}`, true);
    }
  };

  const handleRefresh = async () => {
    setIsRefreshing(true);
    setRefreshSuccess(false);
    try {
      await refreshUsage();
      setRefreshSuccess(true);
      window.setTimeout(() => setRefreshSuccess(false), 1800);
    } catch (err) {
      showToast(`Refresh failed: ${formatError(err)}`, true);
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleWarmupAccount = async (accountId: string, accountName: string) => {
    try {
      setWarmingUpId(accountId);
      await warmupAccount(accountId);
      showToast(`Warm-up sent for ${accountName}`);
    } catch (err) {
      showToast(`Warm-up failed for ${accountName}: ${formatError(err)}`, true);
    } finally {
      setWarmingUpId(null);
    }
  };

  const handleWarmupAll = async () => {
    try {
      setIsWarmingAll(true);
      const summary = await warmupAllAccounts();
      if (summary.total_accounts === 0) showToast("No Codex accounts available for warm-up", true);
      else if (summary.failed_account_ids.length === 0) showToast(`Warm-up sent for ${summary.warmed_accounts} account(s)`);
      else showToast(`Warmed ${summary.warmed_accounts}/${summary.total_accounts}, failed ${summary.failed_account_ids.length}`, true);
    } catch (err) {
      showToast(`Warm-up all failed: ${formatError(err)}`, true);
    } finally {
      setIsWarmingAll(false);
    }
  };

  const openExportModal = async (payloadPromise: Promise<string>, label: string) => {
    setConfigModalMode("slim_export");
    setConfigModalError(null);
    setConfigPayload("");
    setConfigCopied(false);
    setIsConfigModalOpen(true);
    try {
      setConfigPayload(await payloadPromise);
      showToast(label);
    } catch (err) {
      setConfigModalError(formatError(err));
      showToast(`${label} failed`, true);
    }
  };
  const handleExportSlimText = async () => {
    setIsExportingSlim(true);
    await openExportModal(exportAccountsSlimText(), `Slim text exported (${accounts.length} accounts)`);
    setIsExportingSlim(false);
  };
  const handleExportSelectedSlimText = async () => {
    setIsExportingSlim(true);
    await openExportModal(exportSelectedAccountsSlimText(selectedIds), `Slim text exported (${selectedIds.length} selected accounts)`);
    setIsExportingSlim(false);
  };
  const openImportSlimTextModal = () => {
    setConfigModalMode("slim_import");
    setConfigModalError(null);
    setConfigPayload("");
    setConfigCopied(false);
    setIsConfigModalOpen(true);
  };
  const handleImportSlimText = async () => {
    if (!configPayload.trim()) {
      setConfigModalError("Please paste the slim text string first.");
      return;
    }
    try {
      setIsImportingSlim(true);
      const summary = await importAccountsSlimText(configPayload);
      setMaskedAccounts(new Set());
      setIsConfigModalOpen(false);
      showToast(`Imported ${summary.imported_count}, skipped ${summary.skipped_count} (total ${summary.total_in_payload})`);
    } catch (err) {
      setConfigModalError(formatError(err));
      showToast("Slim import failed", true);
    } finally {
      setIsImportingSlim(false);
    }
  };
  const requestBackupPassphrase = async () => {
    const passphrase = window.prompt("Enter a backup passphrase:");
    if (passphrase === null) return null;
    const confirm = window.prompt("Re-enter the backup passphrase:");
    if (confirm === null) return null;
    if (passphrase.trim() !== confirm.trim()) {
      showToast("Backup passphrases did not match", true);
      return null;
    }
    return passphrase;
  };
  const handleExportFullFile = async (selectedOnly = false) => {
    try {
      setIsExportingFull(true);
      const passphrase = await requestBackupPassphrase();
      if (!passphrase) return;
      const selected = await save({
        title: "Export Switchfetcher Backup",
        defaultPath: selectedOnly ? "switchfetcher-selected.swfb" : "switchfetcher-full.swfb",
        filters: [{ name: "Switchfetcher Backup", extensions: ["swfb"] }],
      });
      if (!selected) return;
      if (selectedOnly) await exportSelectedAccountsFullEncryptedFile(selected, passphrase, selectedIds);
      else await exportAccountsFullEncryptedFile(selected, passphrase);
      showToast(selectedOnly ? "Selected encrypted backup exported" : "Full encrypted backup exported");
    } catch {
      showToast("Full export failed", true);
    } finally {
      setIsExportingFull(false);
    }
  };
  const handleImportFullFile = async () => {
    try {
      setIsImportingFull(true);
      const selected = await open({
        multiple: false,
        title: "Import Switchfetcher Backup",
        filters: [{ name: "Switchfetcher Backup", extensions: ["swfb", "cswf"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      const passphrase = window.prompt("Enter the backup passphrase:");
      if (passphrase === null) return;
      const summary = await importAccountsFullEncryptedFile(selected, passphrase);
      showToast(`Imported ${summary.imported_count}, skipped ${summary.skipped_count} (total ${summary.total_in_payload})`);
    } catch {
      showToast("Full import failed", true);
    } finally {
      setIsImportingFull(false);
    }
  };
  const handleRefreshFailed = async () => {
    if (failedAccountIds.length === 0) {
      showToast("No failed accounts to refresh", true);
      return;
    }
    try {
      await refreshSelectedUsage(failedAccountIds);
      showToast(`Refreshed ${failedAccountIds.length} failed account(s)`);
    } catch (err) {
      showToast(`Failed-only refresh failed: ${formatError(err)}`, true);
    }
  };
  const handleProviderVisibility = async (hidden: boolean) => {
    if (filterProvider === "all") return;
    try {
      await setProviderHidden(filterProvider, hidden);
      showToast(hidden ? `Hidden all ${filterProvider} accounts` : `Showing all ${filterProvider} accounts`);
    } catch (err) {
      showToast(`Provider visibility update failed: ${formatError(err)}`, true);
    }
  };
  const handleUpdateTags = async (account: AccountWithUsage) => {
    const next = window.prompt("Comma-separated tags", account.tags.join(", "));
    if (next === null) return;
    const tags = next.split(",").map((tag) => tag.trim()).filter(Boolean);
    try {
      await setAccountTags(account.id, tags);
      showToast(`Updated tags for ${account.name}`);
    } catch (err) {
      showToast(`Failed to update tags: ${formatError(err)}`, true);
    }
  };
  const handleOpenHistory = async () => {
    try {
      setHistoryLoading(true);
      setIsHistoryOpen(true);
      setHistoryEntries(await listAccountHistory(undefined, 40));
    } catch (err) {
      showToast(`Failed to load history: ${formatError(err)}`, true);
    } finally {
      setHistoryLoading(false);
    }
  };
  const handleOpenDiagnostics = async () => {
    try {
      setDiagnosticsLoading(true);
      setIsDiagnosticsOpen(true);
      setDiagnostics(await getDiagnostics());
    } catch (err) {
      showToast(`Failed to load diagnostics: ${formatError(err)}`, true);
    } finally {
      setDiagnosticsLoading(false);
    }
  };
  const handleOpenSettings = async () => {
    try {
      setIsSettingsOpen(true);
      const settings = await loadAppSettings();
      setSettingsDraft(settings);
    } catch (err) {
      showToast(`Failed to load settings: ${formatError(err)}`, true);
    }
  };
  const handleSettingsField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setSettingsDraft((prev) => (prev ? { ...prev, [key]: value } : prev));
  };
  const handleSaveSettings = async () => {
    if (!settingsDraft) return;
    try {
      setSettingsSaving(true);
      const saved = await updateAppSettings(settingsDraft);
      setSettingsDraft(saved);
      showToast("Settings saved");
    } catch (err) {
      showToast(`Failed to save settings: ${formatError(err)}`, true);
    } finally {
      setSettingsSaving(false);
    }
  };
  const handleRequestNotificationPermission = async () => {
    try {
      const state = await requestNotificationPermission();
      showToast(`Notification permission: ${state}`);
    } catch (err) {
      showToast(`Permission request failed: ${formatError(err)}`, true);
    }
  };
  const handleSendTestNotification = async () => {
    try {
      setSendingTestNotification(true);
      await sendTestNotification();
      showToast("Test notification sent");
    } catch (err) {
      showToast(`Test notification failed: ${formatError(err)}`, true);
    } finally {
      setSendingTestNotification(false);
    }
  };

  return (
    <div className="min-h-screen" style={{ background: "var(--color-bg-app)" }}>
      <header
        className="sticky top-0 z-40 border-b"
        style={{
          background: "var(--color-bg-header)",
          borderColor: "var(--color-border)",
        }}
      >
        <div className="max-w-6xl mx-auto px-6 py-4">
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
              <div className="flex items-center gap-3">
                <div
                  className="flex h-10 w-10 items-center justify-center rounded-xl text-lg font-bold"
                  style={{
                    background:
                      "linear-gradient(135deg, var(--color-codex), var(--color-gemini))",
                    color: "#fff",
                  }}
                >
                  S
                </div>
                <div>
                  <div className="flex items-center gap-2 flex-wrap">
                    <h1
                      className="text-xl font-bold tracking-tight"
                      style={{ color: "var(--color-text-primary)" }}
                    >
                      Switchfetcher
                    </h1>
                    {processInfo && (
                      <span
                        className="inline-flex items-center gap-1 rounded-md border px-2 py-0.5 text-xs"
                        style={
                          hasRunningProcesses
                            ? {
                                background: "#fffbeb",
                                color: "#b45309",
                                borderColor: "#fcd34d",
                              }
                            : {
                                background: "#ecfdf5",
                                color: "#047857",
                                borderColor: "#6ee7b7",
                              }
                        }
                      >
                        {hasRunningProcesses ? `${processInfo.count} Codex running` : "0 Codex running"}
                      </span>
                    )}
                  </div>
                  <p className="text-xs" style={{ color: "var(--color-text-secondary)" }}>
                    Filters, history, diagnostics, recommendations and batch actions
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <button
                  onClick={() => setTheme((current) => (current === "light" ? "dark" : "light"))}
                  className="flex h-10 w-10 items-center justify-center rounded-lg sf-btn-secondary"
                  title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
                >
                  {theme === "dark" ? <SunIcon /> : <MoonIcon />}
                </button>
                <button onClick={() => void handleOpenSettings()} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary">Settings</button>
                <button onClick={handleOpenHistory} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary">History</button>
                <button onClick={handleOpenDiagnostics} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary">Diagnostics</button>
                <button onClick={() => setIsRecommendationPickerOpen(true)} className="h-10 px-4 py-2 text-sm font-medium rounded-lg bg-emerald-600 hover:bg-emerald-700 text-white">Switch To Best</button>
                <button onClick={handleRefresh} disabled={isRefreshing} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary disabled:opacity-50">{isRefreshing ? "Refreshing..." : "Refresh All"}</button>
                <button onClick={handleWarmupAll} disabled={isWarmingAll} className="h-10 px-4 py-2 text-sm font-medium rounded-lg sf-btn-secondary disabled:opacity-50">{isWarmingAll ? "Warming..." : "Warm-up All Codex"}</button>
                <div className="relative" ref={actionsMenuRef}>
                  <button onClick={() => setIsActionsMenuOpen((prev) => !prev)} className="h-10 px-4 py-2 text-sm font-medium rounded-lg bg-gray-900 hover:bg-gray-800 text-white">Account ▾</button>
                  {isActionsMenuOpen && <div className="absolute right-0 mt-2 z-50 w-64 rounded-xl p-2 sf-panel">
                    <button onClick={() => { setIsActionsMenuOpen(false); setIsAddModalOpen(true); }} className="w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">+ Add Account</button>
                    <button onClick={() => { setIsActionsMenuOpen(false); void handleExportSlimText(); }} disabled={isExportingSlim} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary disabled:opacity-50">Export Slim Text</button>
                    <button onClick={() => { setIsActionsMenuOpen(false); openImportSlimTextModal(); }} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary">Import Slim Text</button>
                    <button onClick={() => { setIsActionsMenuOpen(false); void handleExportFullFile(false); }} disabled={isExportingFull} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary disabled:opacity-50">Export Full Encrypted File</button>
                    <button onClick={() => { setIsActionsMenuOpen(false); void handleImportFullFile(); }} disabled={isImportingFull} className="mt-1 w-full text-left px-3 py-2 text-sm rounded-lg sf-btn-secondary disabled:opacity-50">Import Full Encrypted File</button>
                  </div>}
                </div>
              </div>
            </div>

            <div className="grid gap-3 md:grid-cols-[auto_1fr_auto_auto]">
              <div className="flex gap-1 rounded-xl p-1 sf-tabs">
                {FILTER_OPTIONS.map((option) => (
                  <button
                    key={option}
                    onClick={() => setFilterProvider(option)}
                    className={`rounded-lg px-3 py-1.5 text-sm font-medium transition-colors ${
                      filterProvider === option ? "sf-tab-active" : ""
                    }`}
                    style={
                      filterProvider === option
                        ? undefined
                        : { color: "var(--color-text-secondary)" }
                    }
                  >
                    {option === "all" ? "All" : option.charAt(0).toUpperCase() + option.slice(1)}
                  </button>
                ))}
              </div>
              <input value={tagFilter} onChange={(event) => setTagFilter(event.target.value)} placeholder={availableTags.length ? `Filter tag (${availableTags.slice(0, 4).join(", ")})` : "Filter by tag"} className="h-10 rounded-lg px-3 text-sm sf-input" />
              <label className="inline-flex h-10 items-center gap-2 rounded-lg px-3 text-sm sf-input" style={{ color: "var(--color-text-secondary)" }}><input type="checkbox" checked={showHidden} onChange={(event) => setShowHidden(event.target.checked)} />Show hidden</label>
              {filterProvider !== "all" ? <div className="flex gap-2"><button onClick={() => void handleProviderVisibility(true)} className="h-10 px-3 rounded-lg text-sm sf-btn-secondary">Hide {filterProvider}</button><button onClick={() => void handleProviderVisibility(false)} className="h-10 px-3 rounded-lg text-sm sf-btn-secondary">Show {filterProvider}</button></div> : null}
            </div>
          </div>
        </div>
      </header>
      <main className="max-w-6xl mx-auto px-6 py-8">
        {loading && accounts.length === 0 ? <div className="flex flex-col items-center justify-center py-20"><div className="animate-spin h-10 w-10 border-2 border-t-transparent rounded-full mb-4" style={{ borderColor: "var(--color-text-primary)", borderTopColor: "transparent" }} /><p style={{ color: "var(--color-text-secondary)" }}>Loading accounts...</p></div> : error ? <div className="text-center py-20"><div className="mb-2 text-red-600">Failed to load accounts</div><p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>{error}</p></div> : filteredAccounts.length === 0 && activeAccounts.length === 0 ? <div className="text-center py-20"><div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl" style={{ background: "var(--color-bg-muted)" }}><span className="text-3xl">👤</span></div><h2 className="mb-2 text-xl font-semibold" style={{ color: "var(--color-text-primary)" }}>No accounts for this filter</h2><p className="mb-6" style={{ color: "var(--color-text-secondary)" }}>Adjust provider/tag filters or add a new account</p><button onClick={() => setIsAddModalOpen(true)} className="rounded-lg px-6 py-3 text-sm font-medium sf-btn-primary">Add Account</button></div> : <div className="space-y-6">
          {activeAccounts.length > 0 ? <section><h2 className="mb-4 text-sm font-medium uppercase tracking-wider" style={{ color: "var(--color-text-secondary)" }}>{activeAccounts.length === 1 ? "Active Account" : "Active Accounts"}</h2><div className="grid grid-cols-1 gap-4 md:grid-cols-2">{activeAccounts.map((account) => <AccountCard key={account.id} account={account} onSwitch={() => undefined} onWarmup={() => handleWarmupAccount(account.id, account.name)} onDelete={() => handleDelete(account.id)} onRefresh={() => refreshSingleUsage(account.id)} onRepair={() => handleRepair(account)} onRename={(newName) => renameAccount(account.id, newName)} onUpdateTags={account.load_state === "ready" ? () => handleUpdateTags(account) : undefined} onToggleSelect={() => toggleSelect(account.id)} selected={selectedAccountIds.has(account.id)} switching={switchingId === account.id} switchDisabled={Boolean(getSwitchDisabledReason(account))} switchDisabledReason={getSwitchDisabledReason(account) ?? undefined} warmingUp={isWarmingAll || warmingUpId === account.id} masked={maskedAccounts.has(account.id)} onToggleMask={() => toggleMask(account.id)} use24hTime={appSettings?.use_24h_time ?? false} />)}</div></section> : null}

          <section className="grid gap-4 lg:grid-cols-3">
            {loadedBestByProvider.map(({ provider, account }) => <div key={provider} className="rounded-2xl p-4 sf-panel"><div className="flex items-center justify-between gap-3"><div><div className="text-xs uppercase tracking-wide" style={{ color: `var(--color-${provider})` }}>{provider}</div><div className="text-base font-semibold" style={{ color: "var(--color-text-primary)" }}>{account ? account.name : "No switchable account"}</div></div><button onClick={() => void handleSwitchBest(provider)} disabled={provider === "gemini" || !account} className="rounded-lg px-3 py-2 text-sm font-medium bg-emerald-50 text-emerald-700 disabled:opacity-50">{provider === "gemini" ? "Usage Only" : "Switch Best"}</button></div><div className="mt-3 text-sm" style={{ color: "var(--color-text-secondary)" }}>{account ? <><div>{formatPlanLabel(account.usage?.plan_type ?? account.plan_type, account.auth_mode)} • {getRemainingPercent(account).toFixed(1)}% remaining</div><div>{formatResetAt(account.usage?.primary_resets_at)}</div></> : <div>No current recommendation</div>}</div></div>)}
          </section>

          {selectedAccounts.length > 0 ? <section className="rounded-2xl border border-amber-200 bg-amber-50 p-4"><div className="flex flex-wrap items-center gap-2 justify-between"><div className="text-sm text-amber-900 font-medium">{selectedAccounts.length} account(s) selected</div><div className="flex flex-wrap gap-2"><button onClick={() => void handleExportSelectedSlimText()} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Export Selected Slim</button><button onClick={() => void handleExportFullFile(true)} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Export Selected Full</button><button onClick={() => setSelectedAccountIds(new Set())} className="px-3 py-2 rounded-lg bg-white border border-amber-200 text-sm text-amber-900">Clear Selection</button></div></div></section> : null}

          <section className="rounded-2xl p-4 sf-panel"><div className="flex flex-wrap items-center justify-between gap-3"><div><div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Batch actions</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Failed refreshes: {failedAccountIds.length} • Hidden filtered by toggle</div></div><div className="flex flex-wrap gap-2"><button onClick={() => void handleRefreshFailed()} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Refresh Failed Only</button><button onClick={() => setSelectedAccountIds(new Set(filteredAccounts.map((account) => account.id)))} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Select Filtered</button></div></div></section>

          {otherAccounts.length > 0 ? <section><div className="mb-4 flex items-center justify-between gap-3"><h2 className="text-sm font-medium uppercase tracking-wider" style={{ color: "var(--color-text-secondary)" }}>Other Accounts ({otherAccounts.length})</h2><select value={otherAccountsSort} onChange={(event) => setOtherAccountsSort(event.target.value as SortMode)} className="appearance-none rounded-xl px-3 py-2 text-sm font-medium sf-input"><option value="deadline_asc">Reset: earliest to latest</option><option value="deadline_desc">Reset: latest to earliest</option><option value="remaining_desc">% remaining: highest to lowest</option><option value="remaining_asc">% remaining: lowest to highest</option></select></div><div className="grid grid-cols-1 gap-4 md:grid-cols-2">{sortedOtherAccounts.map((account) => <AccountCard key={account.id} account={account} onSwitch={() => void handleSwitch(account.id)} onWarmup={() => handleWarmupAccount(account.id, account.name)} onDelete={() => handleDelete(account.id)} onRefresh={() => refreshSingleUsage(account.id)} onRepair={() => handleRepair(account)} onRename={(newName) => renameAccount(account.id, newName)} onUpdateTags={account.load_state === "ready" ? () => handleUpdateTags(account) : undefined} onToggleSelect={() => toggleSelect(account.id)} selected={selectedAccountIds.has(account.id)} switching={switchingId === account.id} switchDisabled={Boolean(getSwitchDisabledReason(account))} switchDisabledReason={getSwitchDisabledReason(account) ?? undefined} warmingUp={isWarmingAll || warmingUpId === account.id} masked={maskedAccounts.has(account.id)} onToggleMask={() => toggleMask(account.id)} use24hTime={appSettings?.use_24h_time ?? false} />)}</div></section> : null}
        </div>}
      </main>

      {refreshSuccess ? <div className="fixed bottom-6 left-1/2 -translate-x-1/2 rounded-lg bg-green-600 px-4 py-3 text-sm text-white shadow-lg">Usage refreshed successfully</div> : null}
      {toast ? <div className={`fixed bottom-20 left-1/2 -translate-x-1/2 px-4 py-3 rounded-lg shadow-lg text-sm ${toast.isError ? "bg-red-600 text-white" : "bg-amber-100 text-amber-900 border border-amber-300"}`}>{toast.message}</div> : null}
      {deleteConfirmId ? <div className="fixed bottom-6 left-1/2 -translate-x-1/2 px-4 py-3 bg-red-600 text-white rounded-lg shadow-lg text-sm">Click delete again to confirm removal</div> : null}

      <AddAccountModal isOpen={isAddModalOpen} onClose={() => setIsAddModalOpen(false)} onImportFile={importFromFile} onImportClaude={importClaudeCredentials} onImportClaudeFromPath={importClaudeCredentialsFromPath} onImportGemini={importGeminiCredentials} onImportGeminiFromPath={importGeminiCredentialsFromPath} onAddGemini={addGeminiAccount} onStartOAuth={startOAuthLogin} onCompleteOAuth={completeOAuthLogin} onCancelOAuth={cancelOAuthLogin} />
      {isConfigModalOpen ? <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay"><div className="mx-4 w-full max-w-2xl rounded-2xl sf-panel"><div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>{configModalMode === "slim_export" ? "Export Slim Text" : "Import Slim Text"}</h2><button onClick={() => setIsConfigModalOpen(false)} style={{ color: "var(--color-text-muted)" }}>✕</button></div><div className="space-y-4 p-5"><textarea value={configPayload} onChange={(event) => setConfigPayload(event.target.value)} readOnly={configModalMode === "slim_export"} placeholder={configModalMode === "slim_export" ? "Export string will appear here" : "Paste config string here"} className="h-48 w-full rounded-lg px-4 py-3 text-sm font-mono sf-input" />{configModalError ? <div className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-600">{configModalError}</div> : null}</div><div className="flex gap-3 border-t p-5" style={{ borderColor: "var(--color-border)" }}><button onClick={() => setIsConfigModalOpen(false)} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-secondary">Close</button>{configModalMode === "slim_export" ? <button onClick={async () => { if (!configPayload) return; try { await navigator.clipboard.writeText(configPayload); setConfigCopied(true); window.setTimeout(() => setConfigCopied(false), 1200); } catch { setConfigModalError("Clipboard unavailable. Please copy manually."); } }} disabled={!configPayload} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary disabled:opacity-50">{configCopied ? "Copied" : "Copy String"}</button> : <button onClick={() => void handleImportSlimText()} disabled={isImportingSlim} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary disabled:opacity-50">{isImportingSlim ? "Importing..." : "Import Missing Accounts"}</button>}</div></div></div> : null}

      {isSettingsOpen ? <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay"><div className="mx-4 w-full max-w-2xl rounded-2xl sf-panel"><div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Settings</h2><button onClick={() => setIsSettingsOpen(false)} style={{ color: "var(--color-text-muted)" }}>✕</button></div><div className="max-h-[70vh] space-y-5 overflow-y-auto p-5">{!settingsDraft ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Loading settings...</p> : <><div className="rounded-xl p-4 space-y-4 sf-panel"><div><div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Background refresh</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>One backend scheduler updates usage cards and tray state.</div></div><label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Enable background refresh</span><input type="checkbox" checked={settingsDraft.background_refresh_enabled} onChange={(event) => handleSettingsField("background_refresh_enabled", event.target.checked)} /></label><label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Refresh interval</span><select value={settingsDraft.base_refresh_interval_seconds} onChange={(event) => handleSettingsField("base_refresh_interval_seconds", Number(event.target.value) as AppSettings["base_refresh_interval_seconds"])} className="h-10 rounded-lg px-3 text-sm sf-input">{REFRESH_INTERVAL_OPTIONS.map((seconds) => <option key={seconds} value={seconds}>{seconds} sec</option>)}</select></label></div><div className="rounded-xl p-4 space-y-4 sf-panel"><div><div className="text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Notifications</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Desktop notifications for reset recovery and quick smoke-testing.</div></div><label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Enable notifications</span><input type="checkbox" checked={settingsDraft.notifications_enabled} onChange={(event) => handleSettingsField("notifications_enabled", event.target.checked)} /></label><label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>Claude reset notifications</span><input type="checkbox" checked={settingsDraft.claude_reset_notifications_enabled} onChange={(event) => handleSettingsField("claude_reset_notifications_enabled", event.target.checked)} /></label><label className="flex items-center justify-between gap-3 text-sm" style={{ color: "var(--color-text-secondary)" }}><span>24-hour clock</span><input type="checkbox" checked={settingsDraft.use_24h_time} onChange={(event) => handleSettingsField("use_24h_time", event.target.checked)} /></label><div className="rounded-lg border p-3 text-sm" style={{ background: "var(--color-bg-muted)", borderColor: "var(--color-border)", color: "var(--color-text-secondary)" }}>Permission state: <span className="font-medium">{notificationPermission}</span></div><div className="flex flex-wrap gap-2"><button onClick={() => void handleRequestNotificationPermission()} className="rounded-lg px-3 py-2 text-sm sf-btn-secondary">Request Permission</button><button onClick={() => void handleSendTestNotification()} disabled={sendingTestNotification} className="rounded-lg px-3 py-2 text-sm sf-btn-primary disabled:opacity-50">{sendingTestNotification ? "Sending..." : "Send Test Notification"}</button></div></div></>}</div><div className="flex gap-3 border-t p-5" style={{ borderColor: "var(--color-border)" }}><button onClick={() => setIsSettingsOpen(false)} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-secondary">Close</button><button onClick={() => void handleSaveSettings()} disabled={!settingsDraft || settingsSaving} className="rounded-lg px-4 py-2.5 text-sm font-medium sf-btn-primary disabled:opacity-50">{settingsSaving ? "Saving..." : "Save Settings"}</button></div></div></div> : null}

      {isHistoryOpen ? <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay"><div className="mx-4 w-full max-w-3xl rounded-2xl sf-panel"><div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Action History</h2><button onClick={() => setIsHistoryOpen(false)} style={{ color: "var(--color-text-muted)" }}>✕</button></div><div className="max-h-[70vh] space-y-3 overflow-y-auto p-5">{historyLoading ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Loading history...</p> : historyEntries.length === 0 ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>No history yet.</p> : historyEntries.map((entry) => <div key={entry.id} className="rounded-xl border p-3" style={{ borderColor: "var(--color-border)" }}><div className="flex items-center justify-between gap-3"><div className="font-medium" style={{ color: "var(--color-text-primary)" }}>{entry.summary}</div><div className="text-xs" style={{ color: "var(--color-text-muted)" }}>{formatHistoryDate(entry.created_at)}</div></div><div className="mt-1 text-sm" style={{ color: "var(--color-text-secondary)" }}>{entry.provider ? `${entry.provider} • ` : ""}{entry.detail ?? entry.kind}</div></div>)}</div></div></div> : null}

      {isDiagnosticsOpen ? <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay"><div className="mx-4 w-full max-w-3xl rounded-2xl sf-panel"><div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Safe Diagnostics</h2><button onClick={() => setIsDiagnosticsOpen(false)} style={{ color: "var(--color-text-muted)" }}>✕</button></div><div className="max-h-[70vh] space-y-5 overflow-y-auto p-5">{diagnosticsLoading || !diagnostics ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>Loading diagnostics...</p> : <><div className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>App version</div><div className="font-semibold" style={{ color: "var(--color-text-primary)" }}>{diagnostics.app_version}</div></div><div className="grid gap-3 md:grid-cols-3">{diagnostics.providers.map((provider) => <div key={provider.provider} className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}><div className="text-sm font-semibold capitalize" style={{ color: "var(--color-text-primary)" }}>{provider.provider}</div><div className="mt-2 text-sm" style={{ color: "var(--color-text-secondary)" }}><div>Supports switch: {provider.supports_switch ? "yes" : "no"}</div><div>Active: {provider.active_account_name ?? "none"}</div><div className="break-all">Path: {provider.credential_path ?? "n/a"}</div></div></div>)}</div><div className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}><div className="mb-3 text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Broken accounts</div><div className="space-y-3">{diagnostics.broken_accounts.length === 0 ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>No broken accounts.</p> : diagnostics.broken_accounts.map((broken) => <div key={broken.account_id} className="rounded-lg border border-amber-100 bg-amber-50 p-3"><div className="font-medium text-amber-900">{broken.name} <span className="text-xs uppercase text-amber-700">{broken.provider}</span></div><div className="text-sm text-amber-800">{broken.reason}</div><div className="mt-1 text-xs text-amber-700">{broken.suggested_source ?? "Manual re-import required"}</div></div>)}</div></div><div className="rounded-xl border p-4" style={{ borderColor: "var(--color-border)" }}><div className="mb-3 text-sm font-semibold" style={{ color: "var(--color-text-primary)" }}>Recent errors</div><div className="space-y-3">{diagnostics.recent_errors.length === 0 ? <p className="text-sm" style={{ color: "var(--color-text-secondary)" }}>No recent errors.</p> : diagnostics.recent_errors.map((errorEntry, index) => <div key={`${errorEntry.created_at}-${index}`} className="rounded-lg border border-red-100 bg-red-50 p-3"><div className="font-medium text-red-900">{errorEntry.summary}</div><div className="text-sm text-red-700">{errorEntry.detail ?? errorEntry.kind}</div><div className="mt-1 text-xs text-red-500">{formatHistoryDate(errorEntry.created_at)}</div></div>)}</div></div></>}</div></div></div> : null}

      {isRecommendationPickerOpen ? <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay"><div className="mx-4 w-full max-w-md rounded-2xl sf-panel"><div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Switch To Best Account</h2><button onClick={() => setIsRecommendationPickerOpen(false)} style={{ color: "var(--color-text-muted)" }}>✕</button></div><div className="space-y-3 p-5">{PROVIDERS.map((provider) => { const account = bestByProvider.get(provider); return <button key={provider} onClick={() => void handleSwitchBest(provider)} className="w-full rounded-xl border px-4 py-3 text-left" style={{ borderColor: "var(--color-border)", background: "var(--color-bg-card)" }}><div className="font-medium capitalize" style={{ color: "var(--color-text-primary)" }}>{provider}</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>{provider === "gemini" ? "Usage-only provider" : account ? `${account.name} • ${formatPlanLabel(account.usage?.plan_type ?? account.plan_type, account.auth_mode)} • ${getRemainingPercent(account).toFixed(1)}% remaining` : "No switchable account available"}</div></button>; })}</div></div></div> : null}
    </div>
  );
}

export default App;
*/

