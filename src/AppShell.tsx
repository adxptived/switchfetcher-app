import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AddAccountModal, ErrorBoundary, Header, AccountGrid } from "./components";
import { ExportImportModal } from "./components/modals/ExportImportModal";
import { DiagnosticsPanel } from "./components/panels/DiagnosticsPanel";
import { HistoryPanel } from "./components/panels/HistoryPanel";
import { SettingsPanel } from "./components/panels/SettingsPanel";
import { useAccounts } from "./hooks/useAccounts";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import {
  checkClaudeProcesses,
  checkCodexProcesses,
  checkGeminiProcesses,
} from "./ipc";
import type {
  AccountAction,
  AccountWithUsage,
  AppSettings,
  ClaudeProcessInfo,
  CodexProcessInfo,
  DiagnosticsSnapshot,
  GeminiProcessInfo,
  Provider,
} from "./types";
import { computeLoadedBestAccount, formatPlanLabel, getRemainingPercent } from "./utils/accounts";
import { formatEnglishDateTime } from "./utils/date";
import { validateRefreshInterval } from "./utils/settings";

type ConfigModalMode = "slim_export" | "slim_import";
type SortMode = "deadline_asc" | "deadline_desc" | "remaining_desc" | "remaining_asc";
const PROVIDERS: Provider[] = ["codex", "claude", "gemini"];
const REPOSITORY_URL = "https://github.com/adxptived/switchfetcher-app";
const LATEST_RELEASE_API_URL = "https://api.github.com/repos/adxptived/switchfetcher-app/releases/latest";

function formatError(err: unknown) {
  if (!err) return "Unknown error";
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string") return err;
  try { return JSON.stringify(err); } catch { return "Unknown error"; }
}

function formatHistoryDate(value: string) {
  return formatEnglishDateTime(value);
}

function normalizeVersion(version: string) {
  return version.trim().replace(/^v/i, "");
}

function compareVersions(current: string, latest: string) {
  const currentParts = normalizeVersion(current).split(".").map((part) => Number.parseInt(part, 10) || 0);
  const latestParts = normalizeVersion(latest).split(".").map((part) => Number.parseInt(part, 10) || 0);
  const length = Math.max(currentParts.length, latestParts.length);
  for (let index = 0; index < length; index += 1) {
    const currentPart = currentParts[index] ?? 0;
    const latestPart = latestParts[index] ?? 0;
    if (currentPart < latestPart) return -1;
    if (currentPart > latestPart) return 1;
  }
  return 0;
}

type UpdateStatus = "checking" | "up_to_date" | "update_available" | "error";

export default function AppShell() {
  const { accounts, appSettings, notificationPermission, loading, error, refreshUsage, refreshSingleUsage, refreshSelectedUsage, warmupAccount, warmupAllAccounts, switchAccount, deleteAccount, deleteAccountsBulk, renameAccount, setAccountTags, listAccountHistory, getBestAccountRecommendation, getDiagnostics, importFromFile, importClaudeCredentials, importClaudeCredentialsFromPath, importGeminiCredentials, importGeminiCredentialsFromPath, addGeminiAccount, repairAccountSecret, exportAccountsSlimText, exportSelectedAccountsSlimText, importAccountsSlimText, exportAccountsFullEncryptedFile, exportSelectedAccountsFullEncryptedFile, importAccountsFullEncryptedFile, loadAppSettings, updateAppSettings, requestNotificationPermission, sendTestNotification, startOAuthLogin, completeOAuthLogin, cancelOAuthLogin } = useAccounts();

  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [isConfigModalOpen, setIsConfigModalOpen] = useState(false);
  const [configModalMode, setConfigModalMode] = useState<ConfigModalMode>("slim_export");
  const [configPayload, setConfigPayload] = useState("");
  const [configModalError, setConfigModalError] = useState<string | null>(null);
  const [configCopied, setConfigCopied] = useState(false);
  const [switchingId, setSwitchingId] = useState<string | null>(null);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [codexProcessInfo, setCodexProcessInfo] = useState<CodexProcessInfo | null>(null);
  const [claudeProcessInfo, setClaudeProcessInfo] = useState<ClaudeProcessInfo | null>(null);
  const [geminiProcessInfo, setGeminiProcessInfo] = useState<GeminiProcessInfo | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [, setIsExportingSlim] = useState(false);
  const [isImportingSlim, setIsImportingSlim] = useState(false);
  const [, setIsExportingFull] = useState(false);
  const [, setIsImportingFull] = useState(false);
  const [isWarmingAll, setIsWarmingAll] = useState(false);
  const [warmingUpId, setWarmingUpId] = useState<string | null>(null);
  const [refreshSuccess, setRefreshSuccess] = useState(false);
  const [toast, setToast] = useState<{ message: string; isError: boolean } | null>(null);
  const [maskedAccounts, setMaskedAccounts] = useState<Set<string>>(new Set());
  const [selectedAccountIds, setSelectedAccountIds] = useState<Set<string>>(new Set());
  const [otherAccountsSort, setOtherAccountsSort] = useState<SortMode>("deadline_asc");
  const [theme, setTheme] = useState<"light" | "dark">(() => {
    const stored = localStorage.getItem("sf-theme");
    return stored === "dark" || stored === "light" ? stored : "light";
  });
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
  const [appVersion, setAppVersion] = useState("");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus | null>(null);
  const [latestVersion, setLatestVersion] = useState<string | null>(null);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const actionsMenuRef = useRef<HTMLDivElement | null>(null);

  const showToast = useCallback((message: string, isError = false) => {
    setToast({ message, isError });
    window.setTimeout(() => setToast(null), 2600);
  }, []);
  const toggleMask = useCallback((accountId: string) => setMaskedAccounts((prev) => {
    const next = new Set(prev);
    if (next.has(accountId)) next.delete(accountId); else next.add(accountId);
    return next;
  }), []);
  const toggleSelect = useCallback((accountId: string) => setSelectedAccountIds((prev) => {
    const next = new Set(prev);
    if (next.has(accountId)) next.delete(accountId); else next.add(accountId);
    return next;
  }), []);

  const checkProcesses = useCallback(async () => {
    try {
      const info = await checkCodexProcesses();
      setCodexProcessInfo(info);
      return info;
    } catch {
      return null;
    }
  }, []);

  const checkClaudeProcessState = useCallback(async () => {
    try {
      setClaudeProcessInfo(await checkClaudeProcesses());
    } catch {
      setClaudeProcessInfo(null);
    }
  }, []);

  const checkGeminiProcessState = useCallback(async () => {
    try {
      setGeminiProcessInfo(await checkGeminiProcesses());
    } catch {
      setGeminiProcessInfo(null);
    }
  }, []);

  useEffect(() => {
    void checkProcesses();
    void checkClaudeProcessState();
    void checkGeminiProcessState();
    const interval = setInterval(() => {
      void checkProcesses();
      void checkClaudeProcessState();
      void checkGeminiProcessState();
    }, 3000);
    return () => clearInterval(interval);
  }, [checkClaudeProcessState, checkGeminiProcessState, checkProcesses]);
  useEffect(() => {
    if (!isActionsMenuOpen) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (actionsMenuRef.current?.contains(event.target as Node)) return;
      setIsActionsMenuOpen(false);
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [isActionsMenuOpen]);
  useEffect(() => { if (appSettings) setSettingsDraft(appSettings); }, [appSettings]);
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("sf-theme", theme);
  }, [theme]);
  useEffect(() => {
    void import("@tauri-apps/api/app")
      .then(({ getVersion }) => getVersion())
      .then(setAppVersion)
      .catch((err) => {
        console.error("Failed to load app version:", err);
      });
  }, []);

  const filteredAccounts = useMemo(() => accounts.filter((account) => !account.hidden), [accounts]);

  const activeAccounts = accounts.filter((account) => account.is_active);
  const otherAccounts = filteredAccounts.filter((account) => !account.is_active);
  const hasRunningProcesses = Boolean(codexProcessInfo?.count);
  const selectedIds = useMemo(() => [...selectedAccountIds], [selectedAccountIds]);
  const selectedAccounts = useMemo(() => filteredAccounts.filter((account) => selectedAccountIds.has(account.id)), [filteredAccounts, selectedAccountIds]);
  const failedAccountIds = useMemo(() => filteredAccounts.filter((account) => account.usage?.error).map((account) => account.id), [filteredAccounts]);
  const loadedBestByProvider = useMemo(() => PROVIDERS.map((provider) => ({ provider, account: computeLoadedBestAccount(accounts, provider) })), [accounts]);
  const bestByProvider = useMemo(() => new Map(loadedBestByProvider.map(({ provider, account }) => [provider, account] as const)), [loadedBestByProvider]);
  const sortedOtherAccounts = useMemo(() => {
    const getReset = (account: AccountWithUsage) => account.usage?.primary_resets_at ?? Number.POSITIVE_INFINITY;
    return [...otherAccounts].sort((left, right) => {
      if (otherAccountsSort === "deadline_asc") return getReset(left) - getReset(right);
      if (otherAccountsSort === "deadline_desc") return getReset(right) - getReset(left);
      if (otherAccountsSort === "remaining_desc") return getRemainingPercent(right) - getRemainingPercent(left);
      return getRemainingPercent(left) - getRemainingPercent(right);
    });
  }, [otherAccounts, otherAccountsSort]);
  const refreshIntervalError = useMemo(() => settingsDraft ? validateRefreshInterval(settingsDraft.base_refresh_interval_seconds) : null, [settingsDraft]);

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
  const handleExportSlimText = async () => { setIsExportingSlim(true); await openExportModal(exportAccountsSlimText(), `Slim text exported (${accounts.length} accounts)`); setIsExportingSlim(false); };
  const handleExportSelectedSlimText = async () => { setIsExportingSlim(true); await openExportModal(exportSelectedAccountsSlimText(selectedIds), `Slim text exported (${selectedIds.length} selected accounts)`); setIsExportingSlim(false); };
  const openImportSlimTextModal = () => { setConfigModalMode("slim_import"); setConfigModalError(null); setConfigPayload(""); setConfigCopied(false); setIsConfigModalOpen(true); };
  const handleImportSlimText = async () => {
    if (!configPayload.trim()) { setConfigModalError("Please paste the slim text string first."); return; }
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
    if (passphrase.trim() !== confirm.trim()) { showToast("Backup passphrases did not match", true); return null; }
    return passphrase;
  };
  const handleExportFullFile = async (selectedOnly = false) => {
    try {
      setIsExportingFull(true);
      const passphrase = await requestBackupPassphrase();
      if (!passphrase) return;
      const selected = await save({ title: "Export Switchfetcher Backup", defaultPath: selectedOnly ? "switchfetcher-selected.swfb" : "switchfetcher-full.swfb", filters: [{ name: "Switchfetcher Backup", extensions: ["swfb"] }] });
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
      const selected = await open({ multiple: false, title: "Import Switchfetcher Backup", filters: [{ name: "Switchfetcher Backup", extensions: ["swfb", "cswf"] }] });
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
    if (failedAccountIds.length === 0) { showToast("No failed accounts to refresh", true); return; }
    try {
      await refreshSelectedUsage(failedAccountIds);
      showToast(`Refreshed ${failedAccountIds.length} failed account(s)`);
    } catch (err) {
      showToast(`Failed-only refresh failed: ${formatError(err)}`, true);
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
  const handleOpenHistory = async () => { try { setHistoryLoading(true); setIsHistoryOpen(true); setHistoryEntries(await listAccountHistory(undefined, 40)); } catch (err) { showToast(`Failed to load history: ${formatError(err)}`, true); } finally { setHistoryLoading(false); } };
  const handleOpenDiagnostics = async () => { try { setDiagnosticsLoading(true); setIsDiagnosticsOpen(true); setDiagnostics(await getDiagnostics()); } catch (err) { showToast(`Failed to load diagnostics: ${formatError(err)}`, true); } finally { setDiagnosticsLoading(false); } };
  const handleRefreshDiagnostics = useCallback(async () => {
    try {
      setDiagnosticsLoading(true);
      setDiagnostics(await getDiagnostics());
      showToast("Diagnostics refreshed");
    } catch (err) {
      showToast(`Failed to refresh diagnostics: ${formatError(err)}`, true);
    } finally {
      setDiagnosticsLoading(false);
    }
  }, [getDiagnostics, showToast]);
  const handleOpenSettings = async () => { try { setIsSettingsOpen(true); const settings = await loadAppSettings(); setSettingsDraft(settings); } catch (err) { showToast(`Failed to load settings: ${formatError(err)}`, true); } };
  const handleSettingsField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setSettingsDraft((prev) => (prev ? { ...prev, [key]: value } : prev));
  const handleSaveSettings = async () => {
    if (!settingsDraft || refreshIntervalError) return;
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
  const handleRequestNotificationPermission = async () => { try { const state = await requestNotificationPermission(); showToast(`Notification permission: ${state}`); } catch (err) { showToast(`Permission request failed: ${formatError(err)}`, true); } };
  const handleSendTestNotification = async () => { try { setSendingTestNotification(true); await sendTestNotification(); showToast("Test notification sent"); } catch (err) { showToast(`Test notification failed: ${formatError(err)}`, true); } finally { setSendingTestNotification(false); } };
  const handleOpenGitHub = useCallback(async () => {
    try {
      await openUrl(REPOSITORY_URL);
    } catch (err) {
      console.error("Failed to open GitHub with opener plugin:", err);
      window.open(REPOSITORY_URL, "_blank", "noopener,noreferrer");
    }
  }, []);
  const handleCheckForUpdates = useCallback(async () => {
    if (!appVersion) {
      setUpdateStatus("error");
      setUpdateMessage("Current app version is unavailable.");
      return;
    }

    try {
      setUpdateStatus("checking");
      setUpdateMessage(null);
      setLatestVersion(null);

      const response = await fetch(LATEST_RELEASE_API_URL, {
        headers: {
          Accept: "application/vnd.github+json",
        },
      });

      if (!response.ok) {
        throw new Error(`GitHub API returned ${response.status}`);
      }

      const data = await response.json() as { tag_name?: string };
      const latestTag = data.tag_name?.trim();
      if (!latestTag) {
        throw new Error("Latest release tag is missing.");
      }

      setLatestVersion(latestTag);
      if (compareVersions(appVersion, latestTag) >= 0) {
        setUpdateStatus("up_to_date");
        setUpdateMessage(`You are up to date. Current version: v${appVersion}.`);
      } else {
        setUpdateStatus("update_available");
        setUpdateMessage(`Update available: ${latestTag} (current: v${appVersion}).`);
      }
    } catch (err) {
      setUpdateStatus("error");
      setUpdateMessage(`Update check failed: ${formatError(err)}`);
    }
  }, [appVersion]);
  const handleDeleteSelected = useCallback(async () => {
    if (selectedIds.length === 0) return;
    try {
      await deleteAccountsBulk(selectedIds);
      setSelectedAccountIds(new Set());
      showToast(`Deleted ${selectedIds.length} account(s)`);
    } catch (err) {
      showToast(`Bulk delete failed: ${formatError(err)}`, true);
    }
  }, [deleteAccountsBulk, selectedIds, showToast]);
  const handleRefreshSelected = useCallback(async () => {
    if (selectedIds.length === 0) return;
    try {
      await refreshSelectedUsage(selectedIds);
      showToast(`Refreshed ${selectedIds.length} selected account(s)`);
    } catch (err) {
      showToast(`Bulk refresh failed: ${formatError(err)}`, true);
    }
  }, [refreshSelectedUsage, selectedIds, showToast]);

  useKeyboardShortcuts({
    onRefreshAll: () => { void handleRefresh(); },
    onEscape: () => {
      setIsConfigModalOpen(false);
      setIsSettingsOpen(false);
      setIsHistoryOpen(false);
      setIsDiagnosticsOpen(false);
      setIsRecommendationPickerOpen(false);
      setIsAddModalOpen(false);
      setIsActionsMenuOpen(false);
    },
  });

  return (
    <div className="min-h-screen" style={{ background: "var(--color-bg-app)" }}>
      <ErrorBoundary fallbackTitle="Header failed">
        <Header appVersion={appVersion} theme={theme} codexProcessInfo={codexProcessInfo} claudeProcessInfo={claudeProcessInfo} geminiProcessInfo={geminiProcessInfo} hasRunningProcesses={hasRunningProcesses} isActionsMenuOpen={isActionsMenuOpen} actionsMenuRef={actionsMenuRef} isRefreshing={isRefreshing} isWarmingAllCodex={isWarmingAll} onThemeToggle={() => setTheme((current) => current === "light" ? "dark" : "light")} onOpenSettings={() => void handleOpenSettings()} onOpenHistory={handleOpenHistory} onOpenDiagnostics={handleOpenDiagnostics} onOpenRecommendationPicker={() => setIsRecommendationPickerOpen(true)} onRefresh={() => void handleRefresh()} onWarmupAllCodex={() => void handleWarmupAll()} onToggleActionsMenu={() => setIsActionsMenuOpen((prev) => !prev)} onOpenAddModal={() => { setIsActionsMenuOpen(false); setIsAddModalOpen(true); }} onExportSlimText={() => { setIsActionsMenuOpen(false); void handleExportSlimText(); }} onOpenImportSlimText={() => { setIsActionsMenuOpen(false); openImportSlimTextModal(); }} onExportFullFile={() => { setIsActionsMenuOpen(false); void handleExportFullFile(false); }} onImportFullFile={() => { setIsActionsMenuOpen(false); void handleImportFullFile(); }} />
      </ErrorBoundary>
      <ErrorBoundary fallbackTitle="Account grid failed">
        <AccountGrid loading={loading} error={error} accounts={accounts} filteredAccounts={filteredAccounts} activeAccounts={activeAccounts} otherAccounts={otherAccounts} sortedOtherAccounts={sortedOtherAccounts} selectedAccounts={selectedAccounts} selectedAccountIds={selectedAccountIds} failedAccountIds={failedAccountIds} loadedBestByProvider={loadedBestByProvider} appSettings={appSettings} otherAccountsSort={otherAccountsSort} bulkMode={false} refreshSuccess={refreshSuccess} maskedAccounts={maskedAccounts} switchingId={switchingId} warmingUpId={warmingUpId} isWarmingAll={isWarmingAll} onOtherAccountsSortChange={setOtherAccountsSort} onAddAccount={() => setIsAddModalOpen(true)} onToggleSelect={toggleSelect} onSwitch={(accountId) => void handleSwitch(accountId)} onWarmupAccount={handleWarmupAccount} onDelete={handleDelete} onRefreshSingle={refreshSingleUsage} onRepair={handleRepair} onRename={renameAccount} onUpdateTags={handleUpdateTags} onToggleMask={toggleMask} getSwitchDisabledReason={getSwitchDisabledReason} onSwitchBest={(provider) => void handleSwitchBest(provider)} onExportSelectedSlimText={() => void handleExportSelectedSlimText()} onExportSelectedFullFile={() => void handleExportFullFile(true)} onClearSelection={() => setSelectedAccountIds(new Set())} onRefreshFailed={() => void handleRefreshFailed()} onSelectFiltered={() => setSelectedAccountIds(new Set(filteredAccounts.map((account) => account.id)))} onRefreshSelected={() => void handleRefreshSelected()} onDeleteSelected={() => void handleDeleteSelected()} />
      </ErrorBoundary>
      {toast ? <div className={`fixed bottom-20 left-1/2 -translate-x-1/2 px-4 py-3 rounded-lg shadow-lg text-sm ${toast.isError ? "bg-red-600 text-white" : "bg-amber-100 text-amber-900 border border-amber-300"}`}>{toast.message}</div> : null}
      {deleteConfirmId ? <div className="fixed bottom-6 left-1/2 -translate-x-1/2 px-4 py-3 bg-red-600 text-white rounded-lg shadow-lg text-sm">Click delete again to confirm removal</div> : null}
      <AddAccountModal isOpen={isAddModalOpen} onClose={() => setIsAddModalOpen(false)} onImportFile={importFromFile} onImportClaude={importClaudeCredentials} onImportClaudeFromPath={importClaudeCredentialsFromPath} onImportGemini={importGeminiCredentials} onImportGeminiFromPath={importGeminiCredentialsFromPath} onAddGemini={addGeminiAccount} onStartOAuth={startOAuthLogin} onCompleteOAuth={completeOAuthLogin} onCancelOAuth={cancelOAuthLogin} />
      <ErrorBoundary fallbackTitle="Export/import modal failed">
        <ExportImportModal isOpen={isConfigModalOpen} mode={configModalMode} payload={configPayload} copied={configCopied} error={configModalError} isImporting={isImportingSlim} onClose={() => setIsConfigModalOpen(false)} onPayloadChange={setConfigPayload} onCopy={async () => { if (!configPayload) return; try { await navigator.clipboard.writeText(configPayload); setConfigCopied(true); window.setTimeout(() => setConfigCopied(false), 1200); } catch { setConfigModalError("Clipboard unavailable. Please copy manually."); } }} onImport={() => void handleImportSlimText()} />
      </ErrorBoundary>
      <ErrorBoundary fallbackTitle="Settings panel failed">
        <SettingsPanel isOpen={isSettingsOpen} settingsDraft={settingsDraft} notificationPermission={notificationPermission} settingsSaving={settingsSaving} sendingTestNotification={sendingTestNotification} refreshIntervalError={refreshIntervalError} appVersion={appVersion} updateStatus={updateStatus} latestVersion={latestVersion} updateMessage={updateMessage} onClose={() => setIsSettingsOpen(false)} onFieldChange={handleSettingsField} onSave={() => void handleSaveSettings()} onRequestNotificationPermission={() => void handleRequestNotificationPermission()} onSendTestNotification={() => void handleSendTestNotification()} onOpenGitHub={() => void handleOpenGitHub()} onCheckForUpdates={() => void handleCheckForUpdates()} />
      </ErrorBoundary>
      <ErrorBoundary fallbackTitle="History panel failed">
        <HistoryPanel isOpen={isHistoryOpen} historyLoading={historyLoading} historyEntries={historyEntries} onClose={() => setIsHistoryOpen(false)} formatHistoryDate={formatHistoryDate} />
      </ErrorBoundary>
      <ErrorBoundary fallbackTitle="Diagnostics panel failed">
        <DiagnosticsPanel isOpen={isDiagnosticsOpen} diagnosticsLoading={diagnosticsLoading} diagnostics={diagnostics} onClose={() => setIsDiagnosticsOpen(false)} onRefreshDiagnostics={() => void handleRefreshDiagnostics()} formatHistoryDate={formatHistoryDate} />
      </ErrorBoundary>
      {isRecommendationPickerOpen ? <div className="fixed inset-0 z-50 flex items-center justify-center sf-overlay"><div className="mx-4 w-full max-w-md rounded-2xl sf-panel"><div className="flex items-center justify-between border-b p-5" style={{ borderColor: "var(--color-border)" }}><h2 className="text-lg font-semibold" style={{ color: "var(--color-text-primary)" }}>Switch To Best Account</h2><button onClick={() => setIsRecommendationPickerOpen(false)} style={{ color: "var(--color-text-muted)" }}>✕</button></div><div className="space-y-3 p-5">{PROVIDERS.map((provider) => { const account = bestByProvider.get(provider); return <button key={provider} onClick={() => void handleSwitchBest(provider)} className="w-full rounded-xl border px-4 py-3 text-left" style={{ borderColor: "var(--color-border)", background: "var(--color-bg-card)" }}><div className="font-medium capitalize" style={{ color: "var(--color-text-primary)" }}>{provider}</div><div className="text-sm" style={{ color: "var(--color-text-secondary)" }}>{provider === "gemini" ? "Usage-only provider" : account ? `${account.name} • ${formatPlanLabel(account.usage?.plan_type ?? account.plan_type, account.auth_mode)} • ${getRemainingPercent(account).toFixed(1)}% remaining` : "No switchable account available"}</div></button>; })}</div></div></div> : null}
    </div>
  );
}
