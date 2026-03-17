//! Core types for Switchfetcher

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Codex,
    Claude,
    Gemini,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountActionKind {
    Switch,
    Import,
    Export,
    RefreshError,
    RefreshRecovered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountAction {
    pub id: String,
    pub account_id: Option<String>,
    pub provider: Option<Provider>,
    pub kind: AccountActionKind,
    pub created_at: DateTime<Utc>,
    pub summary: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountActionSummary {
    pub kind: AccountActionKind,
    pub created_at: DateTime<Utc>,
    pub summary: String,
    pub detail: Option<String>,
    pub is_error: bool,
}

impl AccountActionSummary {
    pub fn from_action(action: &AccountAction) -> Self {
        Self {
            kind: action.kind,
            created_at: action.created_at,
            summary: action.summary.clone(),
            detail: action.detail.clone(),
            is_error: action.is_error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountLoadState {
    Ready,
    NeedsRepair,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider: Provider,
    pub supports_switch: bool,
    pub supports_usage: bool,
    pub supports_warmup: bool,
    pub supports_import_export: bool,
    pub supports_background_watch: bool,
    pub credential_path: Option<String>,
}

impl ProviderCapabilities {
    pub fn from_provider(provider: Provider) -> Self {
        let home = dirs::home_dir();
        let credential_path = home.and_then(|home| match provider {
            Provider::Codex => Some(home.join(".codex").join("auth.json")),
            Provider::Claude => Some(home.join(".claude").join(".credentials.json")),
            Provider::Gemini => Some(home.join(".gemini").join("oauth_creds.json")),
        });

        Self {
            provider,
            supports_switch: matches!(provider, Provider::Codex | Provider::Claude),
            supports_usage: true,
            supports_warmup: matches!(provider, Provider::Codex),
            supports_import_export: true,
            supports_background_watch: matches!(provider, Provider::Claude),
            credential_path: credential_path.map(|path| path.display().to_string()),
        }
    }
}

/// The main storage structure for all accounts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsStore {
    /// Schema version for future migrations
    pub version: u32,
    /// List of all stored accounts
    pub accounts: Vec<StoredAccount>,
    /// Currently active account ID
    #[serde(default)]
    pub active_account_id: Option<String>,
    /// Currently active account ID by provider
    #[serde(default)]
    pub active_account_ids: HashMap<Provider, String>,
    /// Recent account lifecycle events
    #[serde(default)]
    pub history: Vec<AccountAction>,
}

impl Default for AccountsStore {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
            active_account_id: None,
            active_account_ids: std::collections::HashMap::new(),
            history: Vec::new(),
        }
    }
}

impl AccountsStore {
    pub fn active_account_id_for_provider(&self, provider: Provider) -> Option<&str> {
        self.active_account_ids
            .get(&provider)
            .map(String::as_str)
            .or_else(|| {
                self.active_account_id.as_deref().filter(|active_id| {
                    self.accounts
                        .iter()
                        .any(|account| account.id == *active_id && account.provider == provider)
                })
            })
    }

    pub fn set_active_account_for_provider(&mut self, provider: Provider, account_id: String) {
        self.active_account_ids.insert(provider, account_id);
        self.sync_legacy_active_account_id();
    }

    pub fn normalize_active_accounts(&mut self) {
        self.active_account_ids.retain(|provider, account_id| {
            self.accounts
                .iter()
                .any(|account| account.id == *account_id && account.provider == *provider)
        });

        if let Some(legacy_active_id) = self.active_account_id.as_deref() {
            if let Some(account) = self.accounts.iter().find(|account| account.id == legacy_active_id)
            {
                self.active_account_ids
                    .entry(account.provider)
                    .or_insert_with(|| account.id.clone());
            }
        }

        for provider in [Provider::Codex, Provider::Claude, Provider::Gemini] {
            if self.active_account_ids.contains_key(&provider) {
                continue;
            }

            if let Some(account_id) = self
                .accounts
                .iter()
                .find(|account| account.provider == provider)
                .map(|account| account.id.clone())
            {
                self.active_account_ids.insert(provider, account_id);
            }
        }

        self.sync_legacy_active_account_id();
    }

    pub fn sync_legacy_active_account_id(&mut self) {
        self.active_account_id = self
            .active_account_ids
            .get(&Provider::Codex)
            .cloned()
            .or_else(|| self.accounts.first().map(|account| account.id.clone()));
    }
}

/// A stored account with all its metadata and credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    /// Unique identifier (UUID)
    pub id: String,
    /// User-defined display name
    pub name: String,
    /// Account provider
    #[serde(default)]
    pub provider: Provider,
    /// User-defined tags for filtering/grouping
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this account is hidden from the main list by default
    #[serde(default)]
    pub hidden: bool,
    /// Email extracted from ID token (for ChatGPT auth)
    pub email: Option<String>,
    /// Plan type: free, plus, pro, team, business, enterprise, edu
    pub plan_type: Option<String>,
    /// Authentication mode
    pub auth_mode: AuthMode,
    /// Authentication credentials
    pub auth_data: AuthData,
    /// When the account was added
    pub created_at: DateTime<Utc>,
    /// Last time this account was used
    pub last_used_at: Option<DateTime<Utc>>,
}

impl StoredAccount {
    /// Create a new account with API key authentication
    pub fn new_api_key(name: String, api_key: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            provider: Provider::Codex,
            tags: Vec::new(),
            hidden: false,
            email: None,
            plan_type: None,
            auth_mode: AuthMode::ApiKey,
            auth_data: AuthData::ApiKey { key: api_key },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    /// Create a new account with ChatGPT OAuth authentication
    pub fn new_chatgpt(
        name: String,
        email: Option<String>,
        plan_type: Option<String>,
        id_token: String,
        access_token: String,
        refresh_token: String,
        account_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            provider: Provider::Codex,
            tags: Vec::new(),
            hidden: false,
            email,
            plan_type,
            auth_mode: AuthMode::ChatGPT,
            auth_data: AuthData::ChatGPT {
                id_token,
                access_token,
                refresh_token,
                account_id,
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    /// Create a new account with a browser session cookie.
    pub fn new_session_cookie(name: String, provider: Provider, cookie: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            provider,
            tags: Vec::new(),
            hidden: false,
            email: None,
            plan_type: None,
            auth_mode: AuthMode::SessionCookie,
            auth_data: AuthData::SessionCookie { cookie },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    /// Create a new Claude account using OAuth credentials.
    pub fn new_claude_oauth(
        name: String,
        access_token: String,
        refresh_token: String,
        expires_at: i64,
        subscription_type: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            provider: Provider::Claude,
            tags: Vec::new(),
            hidden: false,
            email: None,
            plan_type: subscription_type.clone(),
            auth_mode: AuthMode::ClaudeOAuth,
            auth_data: AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                expires_at,
                subscription_type,
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    /// Create a new Gemini account using OAuth credentials.
    pub fn new_gemini_oauth(
        name: String,
        email: Option<String>,
        access_token: String,
        refresh_token: String,
        id_token: String,
        expiry_date: i64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            provider: Provider::Gemini,
            tags: Vec::new(),
            hidden: false,
            email,
            plan_type: None,
            auth_mode: AuthMode::GeminiOAuth,
            auth_data: AuthData::GeminiOAuth {
                access_token,
                refresh_token,
                id_token,
                expiry_date,
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }
}

/// Authentication mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    /// Using an OpenAI API key
    ApiKey,
    /// Using ChatGPT OAuth tokens
    #[serde(rename = "chat_gpt", alias = "chat_g_p_t")]
    ChatGPT,
    /// Using Claude OAuth tokens
    ClaudeOAuth,
    /// Using Gemini OAuth tokens
    GeminiOAuth,
    /// Using a browser session cookie
    SessionCookie,
}

/// Authentication data (credentials)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthData {
    /// API key authentication
    ApiKey {
        /// The API key
        key: String,
    },
    /// ChatGPT OAuth authentication
    #[serde(rename = "chat_gpt", alias = "chat_g_p_t")]
    ChatGPT {
        /// JWT ID token containing user info
        id_token: String,
        /// Access token for API calls
        access_token: String,
        /// Refresh token for token renewal
        refresh_token: String,
        /// ChatGPT account ID
        account_id: Option<String>,
    },
    /// Claude OAuth authentication
    ClaudeOAuth {
        /// Access token for Claude OAuth API calls
        access_token: String,
        /// Refresh token for token renewal
        refresh_token: String,
        /// Unix timestamp in milliseconds
        expires_at: i64,
        /// Claude subscription label
        subscription_type: Option<String>,
    },
    /// Gemini OAuth authentication
    GeminiOAuth {
        /// Access token for Gemini API calls
        access_token: String,
        /// Refresh token for token renewal
        refresh_token: String,
        /// JWT ID token for user metadata
        id_token: String,
        /// Unix timestamp in milliseconds
        expiry_date: i64,
    },
    /// Browser session cookie authentication
    SessionCookie {
        /// Raw Cookie header value
        cookie: String,
    },
}

// ============================================================================
// Types for Codex's auth.json format (for compatibility)
// ============================================================================

/// The official Codex auth.json format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDotJson {
    /// OpenAI API key (for API key auth mode)
    #[serde(rename = "OPENAI_API_KEY", skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    /// OAuth tokens (for ChatGPT auth mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,
    /// Last token refresh timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

/// Token data stored in auth.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    /// JWT ID token
    pub id_token: String,
    /// Access token
    pub access_token: String,
    /// Refresh token
    pub refresh_token: String,
    /// Account ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

// ============================================================================
// Types for frontend communication
// ============================================================================

/// Account info sent to the frontend (without sensitive data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub tags: Vec<String>,
    pub hidden: bool,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub auth_mode: AuthMode,
    pub capabilities: ProviderCapabilities,
    pub last_action: Option<AccountActionSummary>,
    pub last_refresh_error: Option<AccountActionSummary>,
    pub load_state: AccountLoadState,
    pub unavailable_reason: Option<String>,
    pub repair_hint: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl AccountInfo {
    pub fn from_stored(
        account: &StoredAccount,
        active_id: Option<&str>,
        last_action: Option<AccountActionSummary>,
        last_refresh_error: Option<AccountActionSummary>,
    ) -> Self {
        Self {
            id: account.id.clone(),
            name: account.name.clone(),
            provider: account.provider,
            tags: account.tags.clone(),
            hidden: account.hidden,
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            auth_mode: account.auth_mode,
            capabilities: ProviderCapabilities::from_provider(account.provider),
            last_action,
            last_refresh_error,
            load_state: AccountLoadState::Ready,
            unavailable_reason: None,
            repair_hint: None,
            is_active: active_id == Some(&account.id),
            created_at: account.created_at,
            last_used_at: account.last_used_at,
        }
    }
}

/// Usage information for an account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
    pub today_input_tokens: u64,
    pub today_output_tokens: u64,
    pub today_cache_creation_tokens: u64,
    pub today_cache_read_tokens: u64,
    pub today_cost_usd: f64,
    pub today_session_count: u32,
    pub yesterday_input_tokens: u64,
    pub yesterday_output_tokens: u64,
    pub yesterday_cost_usd: f64,
}

/// Usage information for an account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    /// Account ID
    pub account_id: String,
    /// Plan type
    pub plan_type: Option<String>,
    /// Primary rate limit window usage (percentage 0-100)
    pub primary_used_percent: Option<f64>,
    /// Primary window duration in minutes
    pub primary_window_minutes: Option<i64>,
    /// Primary window reset timestamp (unix seconds)
    pub primary_resets_at: Option<i64>,
    /// Secondary rate limit window usage (percentage 0-100)
    pub secondary_used_percent: Option<f64>,
    /// Secondary window duration in minutes
    pub secondary_window_minutes: Option<i64>,
    /// Secondary window reset timestamp (unix seconds)
    pub secondary_resets_at: Option<i64>,
    /// Whether the account has credits
    pub has_credits: Option<bool>,
    /// Whether credits are unlimited
    pub unlimited_credits: Option<bool>,
    /// Credit balance string (e.g., "$10.50")
    pub credits_balance: Option<String>,
    /// Quota health status badge
    pub quota_status: Option<String>,
    /// Daily token/cost summary
    pub daily_stats: Option<DailyStats>,
    /// Whether the refresh was intentionally skipped instead of failing
    #[serde(default)]
    pub skipped: bool,
    /// Error message if usage fetch failed
    pub error: Option<String>,
}

impl UsageInfo {
    pub fn error(account_id: String, error: String) -> Self {
        Self {
            account_id,
            plan_type: None,
            primary_used_percent: None,
            primary_window_minutes: None,
            primary_resets_at: None,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            quota_status: None,
            daily_stats: None,
            skipped: false,
            error: Some(error),
        }
    }

    pub fn skipped(account_id: String, message: String) -> Self {
        Self {
            account_id,
            plan_type: None,
            primary_used_percent: None,
            primary_window_minutes: None,
            primary_resets_at: None,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            quota_status: None,
            daily_stats: None,
            skipped: true,
            error: Some(message),
        }
    }
}

/// Warm-up execution summary across accounts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupSummary {
    /// Number of accounts that were targeted
    pub total_accounts: usize,
    /// Number of accounts whose warm-up request succeeded
    pub warmed_accounts: usize,
    /// Account IDs whose warm-up request failed
    pub failed_account_ids: Vec<String>,
}

/// Import summary for account config import operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAccountsSummary {
    /// Number of accounts found in the imported payload.
    pub total_in_payload: usize,
    /// Number of accounts actually imported.
    pub imported_count: usize,
    /// Number of accounts skipped because they already exist.
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BestAccountRecommendation {
    pub provider: Provider,
    pub account_id: String,
    pub account_name: String,
    pub plan_type: Option<String>,
    pub score: i64,
    pub reason: String,
    pub remaining_percent: f64,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsProviderState {
    pub provider: Provider,
    pub credential_path: Option<String>,
    pub active_account_name: Option<String>,
    pub active_account_id: Option<String>,
    pub supports_switch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenAccountDiagnostic {
    pub account_id: String,
    pub name: String,
    pub provider: Provider,
    pub reason: String,
    pub suggested_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsSnapshot {
    pub app_version: String,
    pub active_account_id: Option<String>,
    pub providers: Vec<DiagnosticsProviderState>,
    pub broken_accounts: Vec<BrokenAccountDiagnostic>,
    pub recent_errors: Vec<AccountActionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppSettings {
    pub background_refresh_enabled: bool,
    pub base_refresh_interval_seconds: u64,
    pub notifications_enabled: bool,
    pub claude_reset_notifications_enabled: bool,
    #[serde(default)]
    pub use_24h_time: bool,
    #[serde(default)]
    pub usage_alert_threshold: Option<u8>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            background_refresh_enabled: true,
            base_refresh_interval_seconds: 90,
            notifications_enabled: true,
            claude_reset_notifications_enabled: true,
            use_24h_time: false,
            usage_alert_threshold: None,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.base_refresh_interval_seconds = match self.base_refresh_interval_seconds {
            60 | 90 | 120 | 300 => self.base_refresh_interval_seconds,
            _ => Self::default().base_refresh_interval_seconds,
        };
        self.usage_alert_threshold = match self.usage_alert_threshold {
            Some(50..=95) => self.usage_alert_threshold.map(|value| value - (value % 5)),
            _ => None,
        };
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPermissionState {
    Granted,
    Denied,
    Default,
    Unsupported,
}

/// OAuth login information returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthLoginInfo {
    /// The authorization URL to open in browser
    pub auth_url: String,
    /// The local callback port
    pub callback_port: u16,
}

// ============================================================================
// API Response types (from Codex backend)
// ============================================================================

/// Rate limit status from API
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitStatusPayload {
    pub plan_type: String,
    #[serde(default)]
    pub rate_limit: Option<RateLimitDetails>,
    #[serde(default)]
    pub credits: Option<CreditStatusDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitDetails {
    pub primary_window: Option<RateLimitWindow>,
    pub secondary_window: Option<RateLimitWindow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitWindow {
    pub used_percent: f64,
    pub limit_window_seconds: Option<i32>,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditStatusDetails {
    pub has_credits: bool,
    pub unlimited: bool,
    #[serde(default)]
    pub balance: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        AccountsStore, AppSettings, AuthData, AuthMode, NotificationPermissionState, Provider,
        ProviderCapabilities, StoredAccount,
    };

    #[test]
    fn legacy_account_defaults_to_codex_provider() {
        let json = r#"{
            "id":"acc-1",
            "name":"Legacy",
            "email":null,
            "plan_type":null,
            "auth_mode":"chat_gpt",
            "auth_data":{
                "type":"chat_gpt",
                "id_token":"id",
                "access_token":"access",
                "refresh_token":"refresh",
                "account_id":null
            },
            "created_at":"2026-03-15T00:00:00Z",
            "last_used_at":null
        }"#;

        let account: StoredAccount = serde_json::from_str(json).expect("legacy json should parse");

        assert_eq!(account.provider, Provider::Codex);
        assert!(account.tags.is_empty());
        assert!(!account.hidden);
    }

    #[test]
    fn new_claude_oauth_account_uses_claude_provider_and_auth_mode() {
        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );

        assert_eq!(account.provider, Provider::Claude);
        assert_eq!(account.auth_mode, AuthMode::ClaudeOAuth);
        match &account.auth_data {
            AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                expires_at,
                subscription_type,
            } => {
                assert_eq!(access_token, "access");
                assert_eq!(refresh_token, "refresh");
                assert_eq!(*expires_at, 1_763_000_000_000);
                assert_eq!(subscription_type.as_deref(), Some("claude_max"));
            }
            other => panic!("unexpected auth data: {other:?}"),
        }
    }

    #[test]
    fn new_session_cookie_account_uses_requested_provider() {
        let account = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=abc".to_string(),
        );

        assert_eq!(account.provider, Provider::Gemini);
        assert_eq!(account.auth_mode, AuthMode::SessionCookie);
        match &account.auth_data {
            AuthData::SessionCookie { cookie } => assert_eq!(cookie, "__Secure-1PSID=abc"),
            other => panic!("unexpected auth data: {other:?}"),
        }
    }

    #[test]
    fn new_gemini_oauth_account_uses_gemini_provider_and_auth_mode() {
        let account = StoredAccount::new_gemini_oauth(
            "Gemini".to_string(),
            Some("user@example.com".to_string()),
            "access".to_string(),
            "refresh".to_string(),
            "id-token".to_string(),
            1_763_000_000_000,
        );

        assert_eq!(account.provider, Provider::Gemini);
        assert_eq!(account.auth_mode, AuthMode::GeminiOAuth);
        assert_eq!(account.email.as_deref(), Some("user@example.com"));
        match &account.auth_data {
            AuthData::GeminiOAuth {
                access_token,
                refresh_token,
                id_token,
                expiry_date,
            } => {
                assert_eq!(access_token, "access");
                assert_eq!(refresh_token, "refresh");
                assert_eq!(id_token, "id-token");
                assert_eq!(*expiry_date, 1_763_000_000_000);
            }
            other => panic!("unexpected auth data: {other:?}"),
        }
    }

    #[test]
    fn legacy_store_defaults_history_to_empty() {
        let json = r#"{
            "version": 1,
            "accounts": [],
            "active_account_id": null
        }"#;

        let store: AccountsStore = serde_json::from_str(json).expect("legacy store should parse");

        assert!(store.history.is_empty());
    }

    #[test]
    fn normalize_active_accounts_tracks_each_provider_independently() {
        let codex = StoredAccount::new_api_key("Codex".to_string(), "sk-test".to_string());
        let claude = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );
        let mut store = AccountsStore {
            version: 1,
            accounts: vec![codex.clone(), claude.clone()],
            active_account_id: Some(claude.id.clone()),
            active_account_ids: std::collections::HashMap::new(),
            history: Vec::new(),
        };

        store.normalize_active_accounts();

        assert_eq!(
            store.active_account_id_for_provider(Provider::Codex),
            Some(codex.id.as_str())
        );
        assert_eq!(
            store.active_account_id_for_provider(Provider::Claude),
            Some(claude.id.as_str())
        );
        assert_eq!(store.active_account_id, Some(codex.id.clone()));
    }

    #[test]
    fn provider_capabilities_match_switch_expectations() {
        let codex = ProviderCapabilities::from_provider(Provider::Codex);
        let gemini = ProviderCapabilities::from_provider(Provider::Gemini);

        assert!(codex.supports_switch);
        assert!(!gemini.supports_switch);
        assert!(gemini.supports_usage);
    }

    #[test]
    fn app_settings_normalize_invalid_interval_to_default() {
        let settings = AppSettings {
            base_refresh_interval_seconds: 42,
            ..AppSettings::default()
        }
        .normalized();

        assert_eq!(
            settings.base_refresh_interval_seconds,
            AppSettings::default().base_refresh_interval_seconds
        );
    }

    #[test]
    fn app_settings_default_to_12_hour_clock() {
        let settings = AppSettings::default();

        assert!(!settings.use_24h_time);
        assert_eq!(settings.usage_alert_threshold, None);
    }

    #[test]
    fn app_settings_normalize_invalid_threshold_to_none() {
        let settings = AppSettings {
            usage_alert_threshold: Some(42),
            ..AppSettings::default()
        }
        .normalized();

        assert_eq!(settings.usage_alert_threshold, None);
    }

    #[test]
    fn notification_permission_state_serializes_in_snake_case() {
        let encoded = serde_json::to_string(&NotificationPermissionState::Granted)
            .expect("permission state should serialize");

        assert_eq!(encoded, "\"granted\"");
    }
}
