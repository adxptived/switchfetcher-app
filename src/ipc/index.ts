import { invoke } from "@tauri-apps/api/core";
import type {
  AccountAction,
  AccountInfo,
  AppSettings,
  BestAccountRecommendation,
  CodexProcessInfo,
  DiagnosticsSnapshot,
  ImportAccountsSummary,
  NotificationPermissionState,
  OAuthLoginInfo,
  Provider,
  ProviderCapabilities,
  UsageInfo,
  WarmupSummary,
} from "../types";

export const checkCodexProcesses = () => invoke<CodexProcessInfo>("check_codex_processes");
export const listAccounts = () => invoke<AccountInfo[]>("list_accounts");
export const refreshAllAccountsUsage = () => invoke<UsageInfo[]>("refresh_all_accounts_usage");
export const refreshSelectedAccountsUsage = (accountIds: string[]) =>
  invoke<UsageInfo[]>("refresh_selected_accounts_usage", { accountIds });
export const getUsage = (accountId: string) => invoke<UsageInfo>("get_usage", { accountId });
export const warmupAccount = (accountId: string) => invoke<void>("warmup_account", { accountId });
export const warmupAllAccounts = () => invoke<WarmupSummary>("warmup_all_accounts");
export const switchAccount = (accountId: string) => invoke<void>("switch_account", { accountId });
export const deleteAccount = (accountId: string) => invoke<void>("delete_account", { accountId });
export const deleteAccountsBulk = (accountIds: string[]) =>
  invoke<void>("delete_accounts_bulk", { accountIds });
export const renameAccount = (accountId: string, newName: string) =>
  invoke<void>("rename_account", { accountId, newName });
export const addAccountFromFile = (path: string, name: string) =>
  invoke<AccountInfo>("add_account_from_file", { path, name });
export const startLogin = (accountName: string) =>
  invoke<OAuthLoginInfo>("start_login", { accountName });
export const completeLogin = () => invoke<AccountInfo>("complete_login");
export const importClaudeCredentials = (name: string) =>
  invoke<AccountInfo>("import_claude_credentials", { name });
export const importClaudeCredentialsFromPath = (name: string, path: string) =>
  invoke<AccountInfo>("import_claude_credentials_from_path", { name, path });
export const importGeminiCredentials = (name: string) =>
  invoke<AccountInfo>("import_gemini_credentials", { name });
export const importGeminiCredentialsFromPath = (name: string, path: string) =>
  invoke<AccountInfo>("import_gemini_credentials_from_path", { name, path });
export const addSessionCookieAccount = (name: string, cookie: string) =>
  invoke<AccountInfo>("add_session_cookie_account", { name, cookie });
export const exportAccountsSlimText = () => invoke<string>("export_accounts_slim_text");
export const exportSelectedAccountsSlimText = (accountIds: string[]) =>
  invoke<string>("export_selected_accounts_slim_text", { accountIds });
export const importAccountsSlimText = (payload: string) =>
  invoke<ImportAccountsSummary>("import_accounts_slim_text", { payload });
export const exportAccountsFullEncryptedFile = (path: string, passphrase: string) =>
  invoke<void>("export_accounts_full_encrypted_file", { path, passphrase });
export const exportSelectedAccountsFullEncryptedFile = (
  path: string,
  passphrase: string,
  accountIds: string[],
) =>
  invoke<void>("export_selected_accounts_full_encrypted_file", {
    path,
    passphrase,
    accountIds,
  });
export const importAccountsFullEncryptedFile = (path: string, passphrase: string) =>
  invoke<ImportAccountsSummary>("import_accounts_full_encrypted_file", { path, passphrase });
export const cancelLogin = () => invoke<void>("cancel_login");
export const setAccountTags = (accountId: string, tags: string[]) =>
  invoke<AccountInfo>("set_account_tags", { accountId, tags });
export const setProviderHidden = (provider: Provider, hidden: boolean) =>
  invoke<number>("set_provider_hidden", { provider, hidden });
export const listAccountHistory = (accountId?: string, limit?: number) =>
  invoke<AccountAction[]>("list_account_history", { accountId: accountId ?? null, limit });
export const getProviderCapabilities = () =>
  invoke<ProviderCapabilities[]>("get_provider_capabilities");
export const getBestAccountRecommendation = (provider: Provider) =>
  invoke<BestAccountRecommendation | null>("get_best_account_recommendation", { provider });
export const getDiagnostics = () => invoke<DiagnosticsSnapshot>("get_diagnostics");
export const repairAccountSecret = (accountId: string) =>
  invoke<void>("repair_account_secret", { accountId });
export const getAppSettings = () => invoke<AppSettings>("get_app_settings");
export const updateAppSettings = (settings: AppSettings) =>
  invoke<AppSettings>("update_app_settings", { settings });
export const getNotificationPermissionState = () =>
  invoke<NotificationPermissionState>("get_notification_permission_state");
export const requestNotificationPermission = () =>
  invoke<NotificationPermissionState>("request_notification_permission");
export const sendTestNotification = () => invoke<void>("send_test_notification");
