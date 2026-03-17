// Types matching the Rust backend

export type Provider = "codex" | "claude" | "gemini";

export type AuthMode =
  | "api_key"
  | "chat_gpt"
  | "claude_oauth"
  | "gemini_oauth"
  | "session_cookie";

export interface AccountInfo {
  id: string;
  name: string;
  provider: Provider;
  tags: string[];
  hidden: boolean;
  email: string | null;
  plan_type: string | null;
  auth_mode: AuthMode;
  capabilities: ProviderCapabilities;
  last_action: AccountActionSummary | null;
  last_refresh_error: AccountActionSummary | null;
  load_state: "ready" | "needs_repair";
  unavailable_reason: string | null;
  repair_hint: string | null;
  is_active: boolean;
  created_at: string;
  last_used_at: string | null;
}

export type AccountActionKind =
  | "switch"
  | "import"
  | "export"
  | "refresh_error"
  | "refresh_recovered";

export interface AccountActionSummary {
  kind: AccountActionKind;
  created_at: string;
  summary: string;
  detail: string | null;
  is_error: boolean;
}

export interface AccountAction extends AccountActionSummary {
  id: string;
  account_id: string | null;
  provider: Provider | null;
}

export interface ProviderCapabilities {
  provider: Provider;
  supports_switch: boolean;
  supports_usage: boolean;
  supports_warmup: boolean;
  supports_import_export: boolean;
  supports_background_watch: boolean;
  credential_path: string | null;
}

export interface UsageInfo {
  account_id: string;
  plan_type: string | null;
  primary_used_percent: number | null;
  primary_window_minutes: number | null;
  primary_resets_at: number | null;
  secondary_used_percent: number | null;
  secondary_window_minutes: number | null;
  secondary_resets_at: number | null;
  has_credits: boolean | null;
  unlimited_credits: boolean | null;
  credits_balance: string | null;
  quota_status?: "healthy" | "warning" | "critical" | "depleted" | null;
  daily_stats?: DailyStats | null;
  skipped: boolean;
  error: string | null;
}

export interface DailyStats {
  today_input_tokens: number;
  today_output_tokens: number;
  today_cache_creation_tokens: number;
  today_cache_read_tokens: number;
  today_cost_usd: number;
  today_session_count: number;
  yesterday_input_tokens: number;
  yesterday_output_tokens: number;
  yesterday_cost_usd: number;
}

export interface OAuthLoginInfo {
  auth_url: string;
  callback_port: number;
}

export interface AccountWithUsage extends AccountInfo {
  usage?: UsageInfo;
  usageLoading?: boolean;
}

export interface CodexProcessInfo {
  count: number;
  background_count: number;
  can_switch: boolean;
  pids: number[];
}

export interface WarmupSummary {
  total_accounts: number;
  warmed_accounts: number;
  failed_account_ids: string[];
}

export interface ImportAccountsSummary {
  total_in_payload: number;
  imported_count: number;
  skipped_count: number;
}

export interface BestAccountRecommendation {
  provider: Provider;
  account_id: string;
  account_name: string;
  plan_type: string | null;
  score: number;
  reason: string;
  remaining_percent: number;
  resets_at: number | null;
}

export interface DiagnosticsProviderState {
  provider: Provider;
  credential_path: string | null;
  active_account_name: string | null;
  active_account_id: string | null;
  supports_switch: boolean;
}

export interface DiagnosticsSnapshot {
  app_version: string;
  active_account_id: string | null;
  providers: DiagnosticsProviderState[];
  broken_accounts: BrokenAccountDiagnostic[];
  recent_errors: AccountActionSummary[];
}

export interface BrokenAccountDiagnostic {
  account_id: string;
  name: string;
  provider: Provider;
  reason: string;
  suggested_source: string | null;
}

export interface AppSettings {
  background_refresh_enabled: boolean;
  base_refresh_interval_seconds: 60 | 90 | 120 | 300;
  notifications_enabled: boolean;
  claude_reset_notifications_enabled: boolean;
  use_24h_time: boolean;
  usage_alert_threshold: 50 | 55 | 60 | 65 | 70 | 75 | 80 | 85 | 90 | 95 | null;
}

export type NotificationPermissionState =
  | "granted"
  | "denied"
  | "default"
  | "unsupported";
