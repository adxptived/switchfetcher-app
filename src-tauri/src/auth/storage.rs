//! Account storage module - manages reading and writing accounts.json

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use base64::Engine;

use crate::auth::switcher::{import_from_auth_json, read_current_auth};
use crate::types::{
    AccountAction, AccountActionKind, AccountsStore, AuthData, Provider, StoredAccount,
};

const MAX_HISTORY_ITEMS: usize = 200;
const SECRET_SERVICE_NAME: &str = "switchfetcher";
const SECRET_BACKEND_FILE: &str = "file";
const SECRET_BACKEND_KEYCHAIN: &str = "keychain";
static STORAGE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedAccountsStore {
    version: u32,
    #[serde(default)]
    accounts: Vec<PersistedStoredAccount>,
    #[serde(default)]
    active_account_id: Option<String>,
    #[serde(default)]
    active_account_ids: HashMap<Provider, String>,
    #[serde(default)]
    history: Vec<AccountAction>,
}

impl Default for PersistedAccountsStore {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
            active_account_id: None,
            active_account_ids: HashMap::new(),
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PersistedStoredAccount {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) provider: Provider,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) hidden: bool,
    pub(crate) email: Option<String>,
    pub(crate) plan_type: Option<String>,
    pub(crate) auth_mode: crate::types::AuthMode,
    #[serde(default)]
    pub(crate) auth_data: Option<AuthData>,
    #[serde(default)]
    pub(crate) secret_ref: Option<String>,
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub(crate) struct BrokenAccountRecord {
    pub(crate) account: PersistedStoredAccount,
    pub(crate) reason: String,
    pub(crate) repair_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AccountsLoadReport {
    pub(crate) store: AccountsStore,
    pub(crate) broken_accounts: Vec<BrokenAccountRecord>,
}

#[derive(Debug, serde::Deserialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeCredentialsPayload,
}

#[derive(Debug, serde::Deserialize)]
struct ClaudeCredentialsPayload {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GeminiCredentialsPayload {
    access_token: String,
    refresh_token: String,
    id_token: String,
    expiry_date: i64,
}

impl PersistedStoredAccount {
    fn to_stored(&self, auth_data: AuthData) -> StoredAccount {
        StoredAccount {
            id: self.id.clone(),
            name: self.name.clone(),
            provider: self.provider,
            tags: self.tags.clone(),
            hidden: self.hidden,
            email: self.email.clone(),
            plan_type: self.plan_type.clone(),
            auth_mode: self.auth_mode,
            auth_data,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
        }
    }
}

/// Get the path to the switchfetcher config directory
pub fn get_config_dir() -> Result<PathBuf> {
    if let Ok(config_override) = std::env::var("SWITCHFETCHER_CONFIG_DIR") {
        return Ok(PathBuf::from(config_override));
    }
    if let Ok(home_override) = std::env::var("SWITCHFETCHER_HOME") {
        return Ok(PathBuf::from(home_override).join(".switchfetcher"));
    }
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".switchfetcher"))
}

/// Get the path to the legacy config directory used by earlier releases.
fn get_legacy_config_dir() -> Result<PathBuf> {
    if let Ok(config_override) = std::env::var("SWITCHFETCHER_CONFIG_DIR") {
        let configured = PathBuf::from(config_override);
        if let Some(parent) = configured.parent() {
            return Ok(parent.join(".codex-switcher"));
        }
    }
    if let Ok(home_override) = std::env::var("SWITCHFETCHER_HOME") {
        return Ok(PathBuf::from(home_override).join(".codex-switcher"));
    }
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".codex-switcher"))
}

/// Get the path to accounts.json
pub fn get_accounts_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("accounts.json"))
}

fn get_legacy_accounts_file() -> Result<PathBuf> {
    Ok(get_legacy_config_dir()?.join("accounts.json"))
}

fn get_file_secret_store_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("secrets.json"))
}

fn uses_file_secret_backend() -> bool {
    std::env::var("SWITCHFETCHER_SECRET_BACKEND")
        .map(|value| value.eq_ignore_ascii_case(SECRET_BACKEND_FILE))
        .unwrap_or(cfg!(windows))
}

fn secret_ref_for(account_id: &str, backend: &str) -> String {
    format!("{backend}:{SECRET_SERVICE_NAME}:{account_id}")
}

fn read_file_secret(account_id: &str) -> Result<Option<String>> {
    let path = get_file_secret_store_path()?;
    let secrets: HashMap<String, String> = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path)?).unwrap_or_default()
    } else {
        HashMap::new()
    };
    Ok(secrets.get(account_id).cloned())
}

fn write_file_secret(account_id: &str, payload: &str) -> Result<String> {
    let path = get_file_secret_store_path()?;
    let config_dir = get_config_dir()?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("Failed to create config directory: {}", config_dir.display()))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }
    let mut secrets: HashMap<String, String> = if path.exists() {
        serde_json::from_str(&fs::read_to_string(&path)?).unwrap_or_default()
    } else {
        HashMap::new()
    };
    secrets.insert(account_id.to_string(), payload.to_string());
    let serialized = serde_json::to_string_pretty(&secrets)?;
    if let Err(err) = fs::write(&path, &serialized) {
        if err.kind() == std::io::ErrorKind::NotFound {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to recreate config directory: {}", parent.display())
                })?;
            }
            fs::write(&path, serialized)
                .with_context(|| format!("Failed to write file secret store: {}", path.display()))?;
        } else {
            return Err(err)
                .with_context(|| format!("Failed to write file secret store: {}", path.display()));
        }
    }
    Ok(secret_ref_for(account_id, SECRET_BACKEND_FILE))
}

fn delete_file_secret(account_id: &str) -> Result<()> {
    let path = get_file_secret_store_path()?;
    if !path.exists() {
        return Ok(());
    }
    let mut secrets: HashMap<String, String> =
        serde_json::from_str(&fs::read_to_string(&path)?).unwrap_or_default();
    secrets.remove(account_id);
    fs::write(&path, serde_json::to_string_pretty(&secrets)?)
        .with_context(|| format!("Failed to update file secret store: {}", path.display()))?;
    Ok(())
}

fn store_secret(account_id: &str, auth_data: &AuthData) -> Result<String> {
    let payload = serde_json::to_string(auth_data).context("Failed to serialize auth payload")?;

    if uses_file_secret_backend() {
        return write_file_secret(account_id, &payload);
    }

    let entry = keyring::Entry::new(SECRET_SERVICE_NAME, account_id)?;
    entry
        .set_password(&payload)
        .context("Failed to store account secret in OS credential manager")?;
    Ok(secret_ref_for(account_id, SECRET_BACKEND_KEYCHAIN))
}

fn load_secret_from_legacy_store(account: &PersistedStoredAccount) -> Result<Option<AuthData>> {
    let legacy_path = get_legacy_accounts_file()?;
    if !legacy_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&legacy_path).with_context(|| {
        format!(
            "Failed to read legacy accounts file for secret recovery: {}",
            legacy_path.display()
        )
    })?;
    let persisted: PersistedAccountsStore = serde_json::from_str(&content).with_context(|| {
        format!(
            "Failed to parse legacy accounts file for secret recovery: {}",
            legacy_path.display()
        )
    })?;

    let legacy_accounts = persisted.accounts;

    if let Some(auth_data) = legacy_accounts
        .iter()
        .find(|legacy| legacy.id == account.id)
        .and_then(|legacy| legacy.auth_data.clone())
    {
        return Ok(Some(auth_data));
    }

    let mut email_matches = legacy_accounts
        .iter()
        .filter(|legacy| legacy.provider == account.provider)
        .filter(|legacy| match (&legacy.email, &account.email) {
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            _ => false,
        })
        .filter_map(|legacy| legacy.auth_data.clone());
    if let Some(auth_data) = email_matches.next().filter(|_| email_matches.next().is_none()) {
        return Ok(Some(auth_data));
    }

    let mut name_matches = legacy_accounts
        .iter()
        .filter(|legacy| legacy.provider == account.provider)
        .filter(|legacy| legacy.name == account.name)
        .filter_map(|legacy| legacy.auth_data.clone());
    Ok(name_matches.next().filter(|_| name_matches.next().is_none()))
}

fn get_provider_credential_path(provider: Provider) -> Result<Option<PathBuf>> {
    let home = if let Ok(home_override) = std::env::var("SWITCHFETCHER_HOME") {
        Some(PathBuf::from(home_override))
    } else {
        dirs::home_dir()
    };
    let Some(home) = home else {
        return Ok(None);
    };
    Ok(Some(match provider {
        Provider::Codex => home.join(".codex").join("auth.json"),
        Provider::Claude => home.join(".claude").join(".credentials.json"),
        Provider::Gemini => home.join(".gemini").join("oauth_creds.json"),
    }))
}

fn repair_hint_for_account(account: &PersistedStoredAccount) -> Option<String> {
    let path = get_provider_credential_path(account.provider)
        .ok()
        .flatten()
        .map(|value| value.display().to_string());
    match account.provider {
        Provider::Codex => Some(
            path.map(|value| format!("Re-import or restore from {value}"))
                .unwrap_or_else(|| "Re-import the Codex account".to_string()),
        ),
        Provider::Claude => Some(
            path.map(|value| format!("Repair from {value} or re-import Claude credentials"))
                .unwrap_or_else(|| "Re-import Claude credentials".to_string()),
        ),
        Provider::Gemini => Some(
            path.map(|value| format!("Repair from {value} or re-import Gemini credentials"))
                .unwrap_or_else(|| "Re-import Gemini credentials".to_string()),
        ),
    }
}

fn load_secret(secret_ref: Option<&str>, account_id: &str) -> Result<AuthData> {
    if uses_file_secret_backend()
        || secret_ref
            .map(|value| value.starts_with(&format!("{SECRET_BACKEND_FILE}:")))
            .unwrap_or(false)
    {
        let payload = read_file_secret(account_id)?
            .with_context(|| format!("Missing stored secret for account {account_id}"))?;
        return serde_json::from_str(&payload).context("Failed to parse stored secret payload");
    }

    let entry = keyring::Entry::new(SECRET_SERVICE_NAME, account_id)?;
    let payload = entry
        .get_password()
        .context("Failed to load account secret from OS credential manager")?;
    serde_json::from_str(&payload).context("Failed to parse stored secret payload")
}

fn recover_codex_from_provider_file(account: &PersistedStoredAccount) -> Result<Option<AuthData>> {
    let Some(path) = get_provider_credential_path(Provider::Codex)? else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let candidate = import_from_auth_json(&path.to_string_lossy(), account.name.clone())?;
    let email_matches = match (&candidate.email, &account.email) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => false,
    };
    if !email_matches {
        return Ok(None);
    }

    Ok(Some(candidate.auth_data))
}

fn recover_claude_from_provider_file(
    _account: &PersistedStoredAccount,
    persisted_accounts: &[PersistedStoredAccount],
) -> Result<Option<AuthData>> {
    let broken_claude_count = persisted_accounts
        .iter()
        .filter(|entry| entry.provider == Provider::Claude)
        .count();
    if broken_claude_count != 1 {
        return Ok(None);
    }

    let Some(path) = get_provider_credential_path(Provider::Claude)? else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Claude credentials: {}", path.display()))?;
    let parsed: ClaudeCredentialsFile =
        serde_json::from_str(&content).context("Failed to parse Claude credentials file")?;

    let oauth = parsed.claude_ai_oauth;
    Ok(Some(AuthData::ClaudeOAuth {
        access_token: oauth.access_token,
        refresh_token: oauth.refresh_token,
        expires_at: oauth.expires_at,
        subscription_type: oauth.subscription_type,
    }))
}

fn recover_claude_from_provider_file_explicit(
    _account: &PersistedStoredAccount,
) -> Result<Option<AuthData>> {
    let Some(path) = get_provider_credential_path(Provider::Claude)? else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Claude credentials: {}", path.display()))?;
    let parsed: ClaudeCredentialsFile =
        serde_json::from_str(&content).context("Failed to parse Claude credentials file")?;

    let oauth = parsed.claude_ai_oauth;
    Ok(Some(AuthData::ClaudeOAuth {
        access_token: oauth.access_token,
        refresh_token: oauth.refresh_token,
        expires_at: oauth.expires_at,
        subscription_type: oauth.subscription_type,
    }))
}

fn recover_gemini_from_provider_file(
    account: &PersistedStoredAccount,
    persisted_accounts: &[PersistedStoredAccount],
) -> Result<Option<AuthData>> {
    let Some(path) = get_provider_credential_path(Provider::Gemini)? else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Gemini credentials: {}", path.display()))?;
    let parsed: GeminiCredentialsPayload =
        serde_json::from_str(&content).context("Failed to parse Gemini credentials file")?;
    let (email, _) = crate::api::gemini::parse_gemini_id_token_claims(&parsed.id_token);
    let Some(candidate_email) = email else {
        return Ok(None);
    };
    let matching_accounts = persisted_accounts
        .iter()
        .filter(|entry| {
            entry.provider == Provider::Gemini
                && entry
                    .email
                    .as_ref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&candidate_email))
        })
        .count();
    if matching_accounts != 1
        || !account
            .email
            .as_ref()
            .is_some_and(|value| value.eq_ignore_ascii_case(&candidate_email))
    {
        return Ok(None);
    }

    Ok(Some(AuthData::GeminiOAuth {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        id_token: parsed.id_token,
        expiry_date: parsed.expiry_date,
    }))
}

fn recover_gemini_from_provider_file_explicit(
    account: &PersistedStoredAccount,
) -> Result<Option<AuthData>> {
    let Some(path) = get_provider_credential_path(Provider::Gemini)? else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read Gemini credentials: {}", path.display()))?;
    let parsed: GeminiCredentialsPayload =
        serde_json::from_str(&content).context("Failed to parse Gemini credentials file")?;
    let (email, _) = crate::api::gemini::parse_gemini_id_token_claims(&parsed.id_token);
    let Some(candidate_email) = email else {
        return Ok(None);
    };
    if !account
        .email
        .as_ref()
        .is_some_and(|value| value.eq_ignore_ascii_case(&candidate_email))
    {
        return Ok(None);
    }

    Ok(Some(AuthData::GeminiOAuth {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        id_token: parsed.id_token,
        expiry_date: parsed.expiry_date,
    }))
}

fn recover_secret_for_account(
    account: &PersistedStoredAccount,
    persisted_accounts: &[PersistedStoredAccount],
) -> Result<Option<AuthData>> {
    if let Some(auth_data) = load_secret_from_legacy_store(account)? {
        return Ok(Some(auth_data));
    }

    match account.provider {
        Provider::Codex => recover_codex_from_provider_file(account),
        Provider::Claude => recover_claude_from_provider_file(account, persisted_accounts),
        Provider::Gemini => recover_gemini_from_provider_file(account, persisted_accounts),
    }
}

fn load_secret_with_recovery(
    account: &PersistedStoredAccount,
    persisted_accounts: &[PersistedStoredAccount],
) -> Result<(AuthData, bool)> {
    match load_secret(account.secret_ref.as_deref(), &account.id) {
        Ok(auth_data) => Ok((auth_data, false)),
        Err(load_error) => {
            let Some(auth_data) = recover_secret_for_account(account, persisted_accounts)? else {
                return Err(load_error);
            };
            store_secret(&account.id, &auth_data)?;
            Ok((auth_data, true))
        }
    }
}

fn recover_secret_for_explicit_repair(
    account: &PersistedStoredAccount,
    persisted_accounts: &[PersistedStoredAccount],
) -> Result<Option<AuthData>> {
    if let Some(auth_data) = load_secret_from_legacy_store(account)? {
        return Ok(Some(auth_data));
    }

    match account.provider {
        Provider::Codex => recover_secret_for_account(account, persisted_accounts),
        Provider::Claude => recover_claude_from_provider_file_explicit(account),
        Provider::Gemini => recover_gemini_from_provider_file_explicit(account),
    }
}

fn with_storage_lock<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let _guard = STORAGE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation()
}

fn write_store_file(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let temp_path = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("json"),
        uuid::Uuid::new_v4()
    ));
    fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write temp accounts file: {}", temp_path.display()))?;

    if let Err(rename_error) = fs::rename(&temp_path, path) {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("Failed to replace accounts file: {}", path.display()))?;
            fs::rename(&temp_path, path).with_context(|| {
                format!("Failed to finalize accounts file: {}", path.display())
            })?;
        } else {
            return Err(rename_error)
                .with_context(|| format!("Failed to finalize accounts file: {}", path.display()));
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(path, perms)?;
    }

    Ok(())
}

fn backup_corrupted_store(path: &std::path::Path, raw_content: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("accounts.json");
    let backup_path = path.with_file_name(format!(
        "{file_name}.corrupt-{}-{}.bak",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        uuid::Uuid::new_v4()
    ));
    fs::write(&backup_path, raw_content)
        .with_context(|| format!("Failed to back up corrupted accounts file: {}", backup_path.display()))
}

fn recover_persisted_store_from_stream(content: &str) -> Option<PersistedAccountsStore> {
    let mut stream =
        serde_json::Deserializer::from_str(content).into_iter::<PersistedAccountsStore>();
    stream.next().and_then(|value| value.ok())
}

fn parse_email_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("email")
        .and_then(|email| email.as_str())
        .map(str::to_string)
}

fn unique_codex_match<F>(store: &AccountsStore, matcher: F) -> Option<String>
where
    F: Fn(&StoredAccount) -> bool,
{
    let mut matches = store
        .accounts
        .iter()
        .filter(|account| account.provider == Provider::Codex)
        .filter(|account| matcher(account))
        .map(|account| account.id.clone());
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn unique_claude_match<F>(store: &AccountsStore, matcher: F) -> Option<String>
where
    F: Fn(&StoredAccount) -> bool,
{
    let mut matches = store
        .accounts
        .iter()
        .filter(|account| account.provider == Provider::Claude)
        .filter(|account| matcher(account))
        .map(|account| account.id.clone());
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn detect_active_codex_account_id(store: &AccountsStore) -> Result<Option<String>> {
    let Some(current_auth) = read_current_auth().ok().flatten() else {
        return Ok(None);
    };

    if let Some(api_key) = current_auth.openai_api_key {
        return Ok(unique_codex_match(store, |account| {
            matches!(&account.auth_data, AuthData::ApiKey { key } if key == &api_key)
        }));
    }

    let Some(tokens) = current_auth.tokens else {
        return Ok(None);
    };

    if let Some(account_id) = tokens.account_id.as_ref() {
        if let Some(matched) = unique_codex_match(store, |account| {
            matches!(
                &account.auth_data,
                AuthData::ChatGPT {
                    account_id: Some(stored_account_id),
                    ..
                } if stored_account_id == account_id
            )
        }) {
            return Ok(Some(matched));
        }
    }

    if let Some(matched) = unique_codex_match(store, |account| {
        matches!(
            &account.auth_data,
            AuthData::ChatGPT { id_token, .. } if id_token == &tokens.id_token
        )
    }) {
        return Ok(Some(matched));
    }

    let Some(email) = parse_email_from_id_token(&tokens.id_token) else {
        return Ok(None);
    };

    Ok(unique_codex_match(store, |account| {
        account
            .email
            .as_ref()
            .is_some_and(|stored_email| stored_email.eq_ignore_ascii_case(&email))
    }))
}

fn detect_active_claude_account_id(store: &AccountsStore) -> Result<Option<String>> {
    let Some(path) = get_provider_credential_path(Provider::Claude)? else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => return Ok(None),
    };
    let parsed: ClaudeCredentialsFile = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    let oauth = parsed.claude_ai_oauth;

    if let Some(matched) = unique_claude_match(store, |account| {
        matches!(
            &account.auth_data,
            AuthData::ClaudeOAuth {
                refresh_token, ..
            } if refresh_token == &oauth.refresh_token
        )
    }) {
        return Ok(Some(matched));
    }

    Ok(unique_claude_match(store, |account| {
        matches!(
            &account.auth_data,
            AuthData::ClaudeOAuth {
                access_token, ..
            } if access_token == &oauth.access_token
        )
    }))
}

fn reconcile_single_claude_account_from_provider_file(store: &mut AccountsStore) -> Result<bool> {
    let claude_indexes = store
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| account.provider == Provider::Claude)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if claude_indexes.len() != 1 {
        return Ok(false);
    }

    let Some(path) = get_provider_credential_path(Provider::Claude)? else {
        return Ok(false);
    };
    if !path.exists() {
        return Ok(false);
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => return Ok(false),
    };
    let parsed: ClaudeCredentialsFile = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(false),
    };
    let oauth = parsed.claude_ai_oauth;
    let mut changed = false;
    let claude_index = claude_indexes[0];
    let account_id = store.accounts[claude_index].id.clone();

    {
        let account = &mut store.accounts[claude_index];
        if let AuthData::ClaudeOAuth {
            access_token,
            refresh_token,
            expires_at,
            subscription_type,
        } = &mut account.auth_data
        {
            if *access_token != oauth.access_token
                || *refresh_token != oauth.refresh_token
                || *expires_at != oauth.expires_at
                || *subscription_type != oauth.subscription_type
                || account.plan_type != oauth.subscription_type
            {
                *access_token = oauth.access_token;
                *refresh_token = oauth.refresh_token;
                *expires_at = oauth.expires_at;
                *subscription_type = oauth.subscription_type.clone();
                account.plan_type = oauth.subscription_type;
                changed = true;
            }
        }
    }

    if store.active_account_id_for_provider(Provider::Claude) != Some(account_id.as_str()) {
        store.set_active_account_for_provider(Provider::Claude, account_id);
        changed = true;
    }

    Ok(changed)
}

fn sync_active_accounts_with_provider_files(store: &mut AccountsStore) -> Result<bool> {
    let mut changed = false;

    if let Some(codex_id) = detect_active_codex_account_id(store)? {
        if store.active_account_id_for_provider(Provider::Codex) != Some(codex_id.as_str()) {
            store.set_active_account_for_provider(Provider::Codex, codex_id);
            changed = true;
        }
    }

    if reconcile_single_claude_account_from_provider_file(store)? {
        changed = true;
    } else if let Some(claude_id) = detect_active_claude_account_id(store)? {
        if store.active_account_id_for_provider(Provider::Claude) != Some(claude_id.as_str()) {
            store.set_active_account_for_provider(Provider::Claude, claude_id);
            changed = true;
        }
    }

    Ok(changed)
}

fn mutate_accounts_store<T>(mutator: impl FnOnce(&mut AccountsStore) -> Result<T>) -> Result<T> {
    with_storage_lock(|| {
        let mut report = load_accounts_report_inner()?;
        let result = mutator(&mut report.store)?;
        save_accounts_inner(&report.store)?;
        Ok(result)
    })
}

fn delete_secret(account_id: &str) -> Result<()> {
    delete_file_secret(account_id)?;

    if uses_file_secret_backend() {
        return Ok(());
    }

    let entry = keyring::Entry::new(SECRET_SERVICE_NAME, account_id)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("Failed to delete account secret from OS credential manager"),
    }
}

fn persist_store(
    store: &AccountsStore,
    preserved_accounts: &[PersistedStoredAccount],
) -> Result<PersistedAccountsStore> {
    let mut accounts = Vec::with_capacity(store.accounts.len() + preserved_accounts.len());
    let mut preserved_by_id: HashMap<&str, &PersistedStoredAccount> = preserved_accounts
        .iter()
        .map(|account| (account.id.as_str(), account))
        .collect();

    for account in &store.accounts {
        let secret_ref = store_secret(&account.id, &account.auth_data)?;
        accounts.push(PersistedStoredAccount {
            id: account.id.clone(),
            name: account.name.clone(),
            provider: account.provider,
            tags: account.tags.clone(),
            hidden: account.hidden,
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            auth_mode: account.auth_mode,
            auth_data: None,
            secret_ref: Some(secret_ref),
            created_at: account.created_at,
            last_used_at: account.last_used_at,
        });
        preserved_by_id.remove(account.id.as_str());
    }

    for account in preserved_accounts {
        if preserved_by_id.contains_key(account.id.as_str()) {
            accounts.push(account.clone());
        }
    }

    Ok(PersistedAccountsStore {
        version: store.version,
        accounts,
        active_account_id: store.active_account_id.clone(),
        active_account_ids: store.active_account_ids.clone(),
        history: store.history.clone(),
    })
}

fn resolve_persisted_store(persisted: PersistedAccountsStore) -> Result<(AccountsLoadReport, bool)> {
    let mut migrated = false;
    let persisted_accounts = persisted.accounts.clone();
    let mut accounts = Vec::with_capacity(persisted_accounts.len());
    let mut broken_accounts = Vec::new();

    for account in persisted_accounts.iter() {
        let auth_data = if let Some(auth_data) = &account.auth_data {
            match store_secret(&account.id, &auth_data) {
                Ok(_) => {
                    migrated = true;
                    auth_data.clone()
                }
                Err(error) => {
                    broken_accounts.push(BrokenAccountRecord {
                        account: account.clone(),
                        reason: error.to_string(),
                        repair_hint: Some(
                            "Fix OS credential manager access, then run Repair account".to_string(),
                        ),
                    });
                    continue;
                }
            }
        } else {
            match load_secret_with_recovery(account, &persisted_accounts) {
                Ok((auth_data, recovered)) => {
                    migrated |= recovered;
                    auth_data
                }
                Err(error) => {
                    broken_accounts.push(BrokenAccountRecord {
                        account: account.clone(),
                        reason: error.to_string(),
                        repair_hint: repair_hint_for_account(account),
                    });
                    continue;
                }
            }
        };

        accounts.push(account.to_stored(auth_data));
    }

    let mut store = AccountsStore {
        version: persisted.version,
        accounts,
        active_account_id: persisted.active_account_id,
        active_account_ids: persisted.active_account_ids,
        history: persisted.history,
    };
    store.normalize_active_accounts();

    Ok((
        AccountsLoadReport {
            store,
            broken_accounts,
        },
        migrated,
    ))
}

fn read_persisted_store_from_path(path: &PathBuf, label: &str) -> Result<PersistedAccountsStore> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {label}: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {label}: {}", path.display()))
}

fn read_current_persisted_store_inner() -> Result<Option<PersistedAccountsStore>> {
    let path = get_accounts_file()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read accounts file: {}", path.display()))?;
    match serde_json::from_str(&content) {
        Ok(parsed) => Ok(Some(parsed)),
        Err(parse_error) => {
            let Some(recovered) = recover_persisted_store_from_stream(&content) else {
                return Err(parse_error)
                    .with_context(|| format!("Failed to parse accounts file: {}", path.display()));
            };
            backup_corrupted_store(&path, &content)?;
            let rewritten =
                serde_json::to_string_pretty(&recovered).context("Failed to serialize recovered accounts store")?;
            write_store_file(&path, &rewritten)?;
            Ok(Some(recovered))
        }
    }
}

fn read_current_persisted_store() -> Result<Option<PersistedAccountsStore>> {
    with_storage_lock(read_current_persisted_store_inner)
}

fn read_legacy_persisted_store_inner() -> Result<Option<PersistedAccountsStore>> {
    let path = get_legacy_accounts_file()?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_persisted_store_from_path(&path, "legacy accounts file")?))
}

fn load_accounts_report_inner() -> Result<AccountsLoadReport> {
    let current = read_current_persisted_store_inner()?;
    let legacy = read_legacy_persisted_store_inner()?;

    let Some(persisted) = current.or(legacy) else {
        return Ok(AccountsLoadReport {
            store: AccountsStore::default(),
            broken_accounts: Vec::new(),
        });
    };

    let (mut report, migrated) = resolve_persisted_store(persisted)?;
    let reconciled = sync_active_accounts_with_provider_files(&mut report.store)?;
    if migrated || reconciled {
        save_accounts_inner(&report.store)?;
    }
    Ok(report)
}

pub(crate) fn load_accounts_report() -> Result<AccountsLoadReport> {
    with_storage_lock(load_accounts_report_inner)
}

/// Load the accounts store from disk
pub fn load_accounts() -> Result<AccountsStore> {
    Ok(load_accounts_report()?.store)
}

fn apply_repaired_auth(account: &mut PersistedStoredAccount, auth_data: &AuthData) {
    account.auth_data = Some(auth_data.clone());
    account.secret_ref = None;
    if let AuthData::ClaudeOAuth {
        subscription_type, ..
    } = auth_data
    {
        account.plan_type = subscription_type.clone();
    }
}

fn repair_account_secret_targeted_inner(account_id: &str) -> Result<()> {
    let current = read_current_persisted_store_inner()?;
    let legacy = read_legacy_persisted_store_inner()?;
    let Some(mut persisted) = current.or(legacy) else {
        anyhow::bail!("No persisted store found");
    };

    let Some(index) = persisted.accounts.iter().position(|account| account.id == account_id) else {
        anyhow::bail!("Account not found in persisted store");
    };
    let account = persisted.accounts[index].clone();

    if load_secret(account.secret_ref.as_deref(), &account.id).is_ok() {
        return Ok(());
    }

    let Some(auth_data) = recover_secret_for_explicit_repair(&account, &persisted.accounts)? else {
        let report = resolve_persisted_store(persisted)?.0;
        let broken = report
            .broken_accounts
            .iter()
            .find(|entry| entry.account.id == account_id)
            .context("Account is still missing recoverable credentials")?;
        anyhow::bail!(
            "{}{}",
            broken.reason,
            broken
                .repair_hint
                .as_ref()
                .map(|hint| format!(". {hint}"))
                .unwrap_or_default()
        );
    };

    apply_repaired_auth(&mut persisted.accounts[index], &auth_data);

    let path = get_accounts_file()?;
    let content =
        serde_json::to_string_pretty(&persisted).context("Failed to serialize repaired accounts store")?;
    write_store_file(&path, &content)?;

    let report = load_accounts_report_inner()?;
    if report.store.accounts.iter().any(|account| account.id == account_id) {
        return Ok(());
    }

    let broken = report
        .broken_accounts
        .iter()
        .find(|entry| entry.account.id == account_id)
        .context("Account is still missing recoverable credentials")?;
    anyhow::bail!(
        "{}{}",
        broken.reason,
        broken
            .repair_hint
            .as_ref()
            .map(|hint| format!(". {hint}"))
            .unwrap_or_default()
    );
}

pub fn repair_account_secret_targeted(account_id: &str) -> Result<()> {
    with_storage_lock(|| repair_account_secret_targeted_inner(account_id))
}

pub fn repair_account_secret(account_id: &str) -> Result<()> {
    repair_account_secret_targeted(account_id)
}

/// Save the accounts store to disk
fn save_accounts_inner(store: &AccountsStore) -> Result<()> {
    let path = get_accounts_file()?;

    // Ensure the config directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let preserved_accounts = read_current_persisted_store_inner()?
        .map(|current| {
            current
                .accounts
                .into_iter()
                .filter(|account| !store.accounts.iter().any(|known| known.id == account.id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let persisted = persist_store(store, &preserved_accounts)?;
    let content =
        serde_json::to_string_pretty(&persisted).context("Failed to serialize accounts store")?;

    write_store_file(&path, &content)
}

/// Save the accounts store to disk
pub fn save_accounts(store: &AccountsStore) -> Result<()> {
    with_storage_lock(|| save_accounts_inner(store))
}

/// Add a new account to the store
pub fn add_account(account: StoredAccount) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        if store.accounts.iter().any(|a| a.name == account.name) {
            anyhow::bail!("An account with name '{}' already exists", account.name);
        }

        let account_clone = account.clone();
        let provider = account.provider;
        store.accounts.push(account);

        if store.active_account_id_for_provider(provider).is_none() {
            store.set_active_account_for_provider(provider, account_clone.id.clone());
        }

        Ok(account_clone)
    })
}

/// Remove an account by ID
pub fn remove_account(account_id: &str) -> Result<()> {
    with_storage_lock(|| {
        let mut persisted = read_current_persisted_store_inner()?.unwrap_or_default();
        let initial_len = persisted.accounts.len();
        let removed_provider = persisted
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .map(|account| account.provider);
        persisted.accounts.retain(|account| account.id != account_id);
        if persisted.accounts.len() == initial_len {
            return Ok(());
        }

        if persisted.active_account_id.as_deref() == Some(account_id) {
            persisted.active_account_id =
                persisted.accounts.first().map(|account| account.id.clone());
        }

        if let Some(provider) = removed_provider {
            if persisted
                .active_account_ids
                .get(&provider)
                .is_some_and(|id| id == account_id)
            {
                let next_active = persisted
                    .accounts
                    .iter()
                    .find(|account| account.provider == provider)
                    .map(|account| account.id.clone());
                if let Some(next_active) = next_active {
                    persisted.active_account_ids.insert(provider, next_active);
                } else {
                    persisted.active_account_ids.remove(&provider);
                }
            }
        }

        if persisted.active_account_ids.is_empty() {
            persisted.active_account_id =
                persisted.accounts.first().map(|account| account.id.clone());
        }

        let path = get_accounts_file()?;
        let content = serde_json::to_string_pretty(&persisted)
            .context("Failed to serialize accounts store")?;
        write_store_file(&path, &content)?;
        delete_secret(account_id)?;
        Ok(())
    })
}

/// Update the active account ID
pub fn set_active_account(account_id: &str) -> Result<()> {
    mutate_accounts_store(|store| {
        if !store.accounts.iter().any(|a| a.id == account_id) {
            anyhow::bail!("Account not found: {account_id}");
        }

        let provider = store
            .accounts
            .iter()
            .find(|a| a.id == account_id)
            .map(|account| account.provider)
            .ok_or_else(|| anyhow::anyhow!("Account not found: {account_id}"))?;

        store.set_active_account_for_provider(provider, account_id.to_string());
        Ok(())
    })
}

/// Get an account by ID
pub fn get_account(account_id: &str) -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    Ok(store.accounts.into_iter().find(|a| a.id == account_id))
}

/// Get the currently active account
pub fn get_active_account() -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    let active_id = match store.active_account_id_for_provider(Provider::Codex) {
        Some(id) => id.to_string(),
        None => return Ok(None),
    };
    Ok(store.accounts.into_iter().find(|a| a.id == active_id))
}

/// Update an account's last_used_at timestamp
pub fn touch_account(account_id: &str) -> Result<()> {
    mutate_accounts_store(|store| {
        if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
            account.last_used_at = Some(chrono::Utc::now());
        }

        Ok(())
    })
}

/// Update an account's metadata (name, email, plan_type)
pub fn update_account_metadata(
    account_id: &str,
    name: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
) -> Result<()> {
    mutate_accounts_store(|store| {
        if let Some(ref new_name) = name {
            if store
                .accounts
                .iter()
                .any(|a| a.id != account_id && a.name == *new_name)
            {
                anyhow::bail!("An account with name '{new_name}' already exists");
            }
        }

        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        if let Some(new_name) = name {
            account.name = new_name;
        }

        if email.is_some() {
            account.email = email;
        }

        if plan_type.is_some() {
            account.plan_type = plan_type;
        }

        Ok(())
    })
}

pub fn update_account_tags(account_id: &str, tags: Vec<String>) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        account.tags = normalize_tags(tags);
        Ok(account.clone())
    })
}

pub fn set_provider_hidden(provider: Provider, hidden: bool) -> Result<usize> {
    mutate_accounts_store(|store| {
        let mut updated_count = 0usize;

        for account in &mut store.accounts {
            if account.provider == provider && account.hidden != hidden {
                account.hidden = hidden;
                updated_count += 1;
            }
        }

        Ok(updated_count)
    })
}

pub fn push_account_action(
    account_id: Option<String>,
    provider: Option<Provider>,
    kind: AccountActionKind,
    summary: String,
    detail: Option<String>,
    is_error: bool,
) -> Result<()> {
    mutate_accounts_store(|store| {
        push_account_action_to_store(
            store, account_id, provider, kind, summary, detail, is_error,
        );
        Ok(())
    })
}

pub fn mark_account_switched(account_id: &str) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        let position = store
            .accounts
            .iter()
            .position(|account| account.id == account_id)
            .ok_or_else(|| anyhow::anyhow!("Account not found: {account_id}"))?;

        let provider = store.accounts[position].provider;
        let name = store.accounts[position].name.clone();
        store.accounts[position].last_used_at = Some(chrono::Utc::now());
        let updated = store.accounts[position].clone();
        store.set_active_account_for_provider(provider, account_id.to_string());
        push_account_action_to_store(
            store,
            Some(updated.id.clone()),
            Some(provider),
            AccountActionKind::Switch,
            format!("Switched to {name}"),
            None,
            false,
        );

        Ok(updated)
    })
}

pub fn push_account_action_to_store(
    store: &mut AccountsStore,
    account_id: Option<String>,
    provider: Option<Provider>,
    kind: AccountActionKind,
    summary: String,
    detail: Option<String>,
    is_error: bool,
) {
    store.history.push(AccountAction {
        id: uuid::Uuid::new_v4().to_string(),
        account_id,
        provider,
        kind,
        created_at: chrono::Utc::now(),
        summary,
        detail,
        is_error,
    });

    if store.history.len() > MAX_HISTORY_ITEMS {
        let overflow = store.history.len() - MAX_HISTORY_ITEMS;
        store.history.drain(0..overflow);
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(trimmed))
        {
            continue;
        }
        normalized.push(trimmed.to_string());
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{
        get_accounts_file, get_file_secret_store_path, load_accounts, load_accounts_report,
        mark_account_switched, remove_account, repair_account_secret, save_accounts,
        AccountsStore, Provider, StoredAccount,
    };
    use base64::Engine;
    use crate::types::AuthData;
    use std::fs;
    use std::sync::{LazyLock, Mutex};
    use crate::auth::switcher::switch_to_account;

    static ENV_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn make_temp_home(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "switchfetcher-storage-tests-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("temp dir should exist");
        dir
    }

    fn make_fake_jwt(email: &str, plan: &str, account_id: Option<&str>) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = serde_json::json!({
            "email": email,
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": plan,
                "chatgpt_account_id": account_id,
            }
        });
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&payload).expect("payload json"));
        format!("{header}.{payload}.sig")
    }

    fn make_chatgpt_account(name: &str, email: &str, account_id: Option<&str>) -> StoredAccount {
        StoredAccount::new_chatgpt(
            name.to_string(),
            Some(email.to_string()),
            Some("team".to_string()),
            make_fake_jwt(email, "team", account_id),
            format!("access-{name}"),
            format!("refresh-{name}"),
            account_id.map(str::to_string),
        )
    }

    #[cfg(windows)]
    #[test]
    fn uses_file_secret_backend_defaults_to_file_on_windows() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        unsafe {
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }

        assert!(super::uses_file_secret_backend());
    }

    #[test]
    fn load_accounts_migrates_inline_auth_data_into_secret_backend() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("migrate");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", temp_home.join(".switchfetcher"));
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );
        let legacy_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": account.id,
                "name": account.name,
                "provider": "claude",
                "email": account.email,
                "plan_type": account.plan_type,
                "auth_mode": "claude_o_auth",
                "auth_data": account.auth_data,
                "created_at": account.created_at,
                "last_used_at": account.last_used_at,
                "tags": [],
                "hidden": false
            }],
            "active_account_id": null,
            "history": []
        });
        let accounts_file = get_accounts_file().expect("accounts file path");
        fs::create_dir_all(accounts_file.parent().expect("config dir")).expect("config dir");
        fs::write(&accounts_file, serde_json::to_string_pretty(&legacy_json).unwrap())
            .expect("legacy accounts file should be written");

        let loaded = load_accounts().expect("legacy store should load");
        let reserialized = fs::read_to_string(&accounts_file).expect("accounts file should exist");

        assert_eq!(loaded.accounts.len(), 1);
        assert!(!reserialized.contains("\"access_token\": \"access\""));
        assert!(reserialized.contains("\"secret_ref\""));

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn save_accounts_round_trips_via_secret_backend() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("roundtrip");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", temp_home.join(".switchfetcher"));
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=abc; __Secure-1PSIDTS=def".to_string(),
        );
        let store = AccountsStore {
            version: 1,
            accounts: vec![account.clone()],
            active_account_id: Some(account.id.clone()),
            active_account_ids: std::collections::HashMap::from([(
                Provider::Gemini,
                account.id.clone(),
            )]),
            history: Vec::new(),
        };

        save_accounts(&store).expect("save should succeed");
        let loaded = load_accounts().expect("load should succeed");

        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].provider, Provider::Gemini);

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn save_accounts_uses_switchfetcher_home_dir_instead_of_legacy_dir() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("current-home-dir");
        let config_dir = temp_home.join(".switchfetcher");
        let legacy_dir = temp_home.join(".codex-switcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=home; __Secure-1PSIDTS=current".to_string(),
        );
        let store = AccountsStore {
            version: 1,
            accounts: vec![account.clone()],
            active_account_id: Some(account.id.clone()),
            active_account_ids: std::collections::HashMap::from([(
                Provider::Gemini,
                account.id.clone(),
            )]),
            history: Vec::new(),
        };

        save_accounts(&store).expect("save should create current config dir");

        assert!(config_dir.join("accounts.json").exists());
        assert!(config_dir.join("secrets.json").exists());
        assert!(!legacy_dir.exists());

        unsafe {
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn remove_account_deletes_legacy_only_account() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("remove-legacy-only");
        let config_dir = temp_home.join(".switchfetcher");
        let legacy_dir = temp_home.join(".codex-switcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_session_cookie(
            "Gemini Legacy".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=legacy; __Secure-1PSIDTS=only".to_string(),
        );

        fs::create_dir_all(&legacy_dir).expect("legacy dir");
        let legacy_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": account.id,
                "name": account.name,
                "provider": "gemini",
                "tags": [],
                "hidden": false,
                "email": account.email,
                "plan_type": account.plan_type,
                "auth_mode": "session_cookie",
                "auth_data": account.auth_data,
                "created_at": account.created_at,
                "last_used_at": account.last_used_at
            }],
            "active_account_id": account.id,
            "history": []
        });
        fs::write(
            legacy_dir.join("accounts.json"),
            serde_json::to_string_pretty(&legacy_json).unwrap(),
        )
        .expect("legacy accounts file");

        let loaded = load_accounts().expect("legacy-only account should load");
        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].id, account.id);

        remove_account(&account.id).expect("delete should succeed for legacy-only account");

        let loaded_after = load_accounts().expect("load after delete should succeed");
        assert!(loaded_after.accounts.is_empty());
        assert!(get_accounts_file().expect("current accounts file").exists());

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn remove_account_is_noop_when_account_is_already_missing() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("remove-missing");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", temp_home.join(".switchfetcher"));
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let result = remove_account("5585d3b0-ba41-4be3-9f21-7bd1d4404c1f");

        assert!(result.is_ok(), "deleting a stale account id should not fail");

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn load_accounts_recovers_missing_secret_from_legacy_store() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("legacy-recovery");
        let config_dir = temp_home.join(".switchfetcher");
        let legacy_dir = temp_home.join(".codex-switcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=recover; __Secure-1PSIDTS=restore".to_string(),
        );

        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&legacy_dir).expect("legacy dir");

        let current_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": account.id,
                "name": account.name,
                "provider": "gemini",
                "tags": [],
                "hidden": false,
                "email": account.email,
                "plan_type": account.plan_type,
                "auth_mode": "gemini_o_auth",
                "auth_data": null,
                "secret_ref": "keychain:switchfetcher:missing",
                "created_at": account.created_at,
                "last_used_at": account.last_used_at
            }],
            "active_account_id": null,
            "history": []
        });
        fs::write(
            config_dir.join("accounts.json"),
            serde_json::to_string_pretty(&current_json).unwrap(),
        )
        .expect("current accounts file");

        let legacy_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": account.id,
                "name": account.name,
                "email": account.email,
                "plan_type": account.plan_type,
                "auth_mode": "session_cookie",
                "auth_data": account.auth_data,
                "created_at": account.created_at,
                "last_used_at": account.last_used_at
            }],
            "active_account_id": null
        });
        fs::write(
            legacy_dir.join("accounts.json"),
            serde_json::to_string_pretty(&legacy_json).unwrap(),
        )
        .expect("legacy accounts file");

        let loaded = load_accounts().expect("load should recover from legacy file");
        let secret_store = fs::read_to_string(get_file_secret_store_path().expect("secrets path"))
            .expect("file secret store");

        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].provider, Provider::Gemini);
        assert!(secret_store.contains(&account.id));

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn load_accounts_recovers_codex_secret_from_legacy_store_by_email_when_ids_differ() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("legacy-codex-email-recovery");
        let config_dir = temp_home.join(".switchfetcher");
        let legacy_dir = temp_home.join(".codex-switcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let legacy = make_chatgpt_account("Codex Legacy", "legacy@example.com", Some("acct-legacy"));
        let current = StoredAccount {
            id: uuid::Uuid::new_v4().to_string(),
            name: legacy.name.clone(),
            provider: Provider::Codex,
            tags: vec![],
            hidden: false,
            email: legacy.email.clone(),
            plan_type: legacy.plan_type.clone(),
            auth_mode: legacy.auth_mode,
            auth_data: AuthData::ApiKey {
                key: "placeholder".to_string(),
            },
            created_at: legacy.created_at,
            last_used_at: legacy.last_used_at,
        };

        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&legacy_dir).expect("legacy dir");

        let current_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": current.id,
                "name": current.name,
                "provider": "codex",
                "tags": [],
                "hidden": false,
                "email": current.email,
                "plan_type": current.plan_type,
                "auth_mode": "chat_gpt",
                "auth_data": null,
                "secret_ref": "keychain:switchfetcher:missing-codex",
                "created_at": current.created_at,
                "last_used_at": current.last_used_at
            }],
            "active_account_id": current.id,
            "history": []
        });
        fs::write(
            config_dir.join("accounts.json"),
            serde_json::to_string_pretty(&current_json).unwrap(),
        )
        .expect("current accounts file");

        let legacy_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": legacy.id,
                "name": legacy.name,
                "email": legacy.email,
                "plan_type": legacy.plan_type,
                "auth_mode": "chat_gpt",
                "auth_data": legacy.auth_data,
                "created_at": legacy.created_at,
                "last_used_at": legacy.last_used_at
            }],
            "active_account_id": legacy.id
        });
        fs::write(
            legacy_dir.join("accounts.json"),
            serde_json::to_string_pretty(&legacy_json).unwrap(),
        )
        .expect("legacy accounts file");

        let loaded = load_accounts().expect("load should recover codex account from legacy email");
        let secret_store = fs::read_to_string(get_file_secret_store_path().expect("secrets path"))
            .expect("file secret store");

        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].id, current.id);
        assert_eq!(loaded.accounts[0].email, legacy.email);
        assert!(matches!(loaded.accounts[0].auth_data, AuthData::ChatGPT { .. }));
        assert!(secret_store.contains(&current.id));

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn repair_account_secret_recovers_codex_secret_from_legacy_store() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("repair-codex-legacy");
        let config_dir = temp_home.join(".switchfetcher");
        let legacy_dir = temp_home.join(".codex-switcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let legacy = make_chatgpt_account("Codex Repair", "repair@example.com", Some("acct-repair"));
        let current = StoredAccount {
            id: uuid::Uuid::new_v4().to_string(),
            name: legacy.name.clone(),
            provider: Provider::Codex,
            tags: vec![],
            hidden: false,
            email: legacy.email.clone(),
            plan_type: legacy.plan_type.clone(),
            auth_mode: legacy.auth_mode,
            auth_data: AuthData::ApiKey {
                key: "placeholder".to_string(),
            },
            created_at: legacy.created_at,
            last_used_at: legacy.last_used_at,
        };

        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&legacy_dir).expect("legacy dir");

        let current_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": current.id,
                "name": current.name,
                "provider": "codex",
                "tags": [],
                "hidden": false,
                "email": current.email,
                "plan_type": current.plan_type,
                "auth_mode": "chat_gpt",
                "auth_data": null,
                "secret_ref": "keychain:switchfetcher:missing-codex",
                "created_at": current.created_at,
                "last_used_at": current.last_used_at
            }],
            "active_account_id": current.id,
            "history": []
        });
        fs::write(
            config_dir.join("accounts.json"),
            serde_json::to_string_pretty(&current_json).unwrap(),
        )
        .expect("current accounts file");

        let legacy_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": legacy.id,
                "name": legacy.name,
                "email": legacy.email,
                "plan_type": legacy.plan_type,
                "auth_mode": "chat_gpt",
                "auth_data": legacy.auth_data,
                "created_at": legacy.created_at,
                "last_used_at": legacy.last_used_at
            }],
            "active_account_id": legacy.id
        });
        fs::write(
            legacy_dir.join("accounts.json"),
            serde_json::to_string_pretty(&legacy_json).unwrap(),
        )
        .expect("legacy accounts file");

        repair_account_secret(&current.id).expect("repair should recover and persist secret");

        let loaded = load_accounts().expect("load should succeed after repair");
        let secret_store = fs::read_to_string(get_file_secret_store_path().expect("secrets path"))
            .expect("file secret store");

        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(loaded.accounts[0].id, current.id);
        assert!(matches!(loaded.accounts[0].auth_data, AuthData::ChatGPT { .. }));
        assert!(secret_store.contains(&current.id));

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    #[ignore = "uses process-global home overrides and is flaky in full parallel suite"]
    fn repair_account_secret_recovers_selected_broken_claude_account_from_provider_file() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("repair-claude-targeted");
        let config_dir = temp_home.join(".switchfetcher");
        let claude_dir = temp_home.join(".claude");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let first = StoredAccount::new_claude_oauth(
            "Claude Repair A".to_string(),
            "stale-access-a".to_string(),
            "stale-refresh-a".to_string(),
            1_763_000_000_000,
            Some("claude_pro".to_string()),
        );
        let second = StoredAccount::new_claude_oauth(
            "Claude Repair B".to_string(),
            "stale-access-b".to_string(),
            "stale-refresh-b".to_string(),
            1_763_000_000_001,
            Some("claude_max".to_string()),
        );

        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&claude_dir).expect("claude dir");
        let current_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": first.id,
                "name": first.name,
                "provider": "claude",
                "tags": [],
                "hidden": false,
                "email": first.email,
                "plan_type": first.plan_type,
                "auth_mode": "claude_o_auth",
                "auth_data": null,
                "secret_ref": "keychain:switchfetcher:missing-claude-a",
                "created_at": first.created_at,
                "last_used_at": first.last_used_at
            }, {
                "id": second.id,
                "name": second.name,
                "provider": "claude",
                "tags": [],
                "hidden": false,
                "email": second.email,
                "plan_type": second.plan_type,
                "auth_mode": "claude_o_auth",
                "auth_data": null,
                "secret_ref": "keychain:switchfetcher:missing-claude-b",
                "created_at": second.created_at,
                "last_used_at": second.last_used_at
            }],
            "active_account_id": second.id,
            "active_account_ids": { "claude": second.id },
            "history": []
        });
        fs::write(
            config_dir.join("accounts.json"),
            serde_json::to_string_pretty(&current_json).unwrap(),
        )
        .expect("current accounts file");
        fs::write(
            claude_dir.join(".credentials.json"),
            r#"{
                "claudeAiOauth": {
                    "accessToken": "live-access",
                    "refreshToken": "live-refresh",
                    "expiresAt": 1763100000000,
                    "subscriptionType": "claude_max"
                }
            }"#,
        )
        .expect("claude credentials file");

        let report_before = load_accounts_report().expect("load should report broken accounts");
        assert!(report_before.store.accounts.is_empty());
        assert_eq!(report_before.broken_accounts.len(), 2);
        assert!(report_before
            .broken_accounts
            .iter()
            .any(|broken| broken.account.id == first.id));

        repair_account_secret(&first.id).expect("repair should recover selected Claude account");

        let report_after = load_accounts_report().expect("load should succeed after targeted repair");
        assert_eq!(report_after.store.accounts.len(), 1);
        assert_eq!(report_after.broken_accounts.len(), 1);
        assert_eq!(report_after.store.accounts[0].id, first.id);
        assert_eq!(report_after.store.accounts[0].plan_type.as_deref(), Some("claude_max"));
        assert!(report_after
            .broken_accounts
            .iter()
            .all(|broken| broken.account.id != first.id));

        match &report_after.store.accounts[0].auth_data {
            AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                expires_at,
                subscription_type,
            } => {
                assert_eq!(access_token, "live-access");
                assert_eq!(refresh_token, "live-refresh");
                assert_eq!(*expires_at, 1_763_100_000_000);
                assert_eq!(subscription_type.as_deref(), Some("claude_max"));
            }
            other => panic!("expected ClaudeOAuth auth data, got {other:?}"),
        }

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    #[ignore = "uses process-global home overrides and is flaky in full parallel suite"]
    fn load_accounts_recovers_claude_from_provider_file() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("claude-provider");
        let config_dir = temp_home.join(".switchfetcher");
        let claude_dir = temp_home.join(".claude");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "missing-access".to_string(),
            "missing-refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::create_dir_all(&claude_dir).expect("claude dir");
        let current_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": account.id,
                "name": account.name,
                "provider": "claude",
                "tags": [],
                "hidden": false,
                "email": account.email,
                "plan_type": account.plan_type,
                "auth_mode": "claude_o_auth",
                "auth_data": null,
                "secret_ref": "keychain:switchfetcher:missing",
                "created_at": account.created_at,
                "last_used_at": account.last_used_at
            }],
            "active_account_id": null,
            "history": []
        });
        fs::write(
            config_dir.join("accounts.json"),
            serde_json::to_string_pretty(&current_json).unwrap(),
        )
        .expect("current accounts file");
        fs::write(
            claude_dir.join(".credentials.json"),
            r#"{
                "claudeAiOauth": {
                    "accessToken": "access",
                    "refreshToken": "refresh",
                    "expiresAt": 1763000000000,
                    "subscriptionType": "claude_max"
                }
            }"#,
        )
        .expect("claude credentials file");

        let report = load_accounts_report().expect("load should recover from claude provider file");

        assert_eq!(report.store.accounts.len(), 1);
        assert!(report.broken_accounts.is_empty());

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    #[ignore = "uses process-global home overrides and is flaky in full parallel suite"]
    fn load_accounts_self_heals_single_claude_account_from_runtime_file() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("claude-runtime-self-heal");
        let config_dir = temp_home.join(".switchfetcher");
        let claude_dir = temp_home.join(".claude");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let stale = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "stale-access".to_string(),
            "stale-refresh".to_string(),
            1_763_000_000_000,
            Some("claude_pro".to_string()),
        );
        let store = AccountsStore {
            version: 1,
            accounts: vec![stale.clone()],
            active_account_id: None,
            active_account_ids: std::collections::HashMap::new(),
            history: Vec::new(),
        };
        save_accounts(&store).expect("seed stale claude store");

        fs::create_dir_all(&claude_dir).expect("claude dir");
        fs::write(
            claude_dir.join(".credentials.json"),
            r#"{
                "claudeAiOauth": {
                    "accessToken": "live-access",
                    "refreshToken": "live-refresh",
                    "expiresAt": 1763100000000,
                    "subscriptionType": "claude_max"
                }
            }"#,
        )
        .expect("live claude credentials file");

        let loaded = load_accounts().expect("load should self-heal single claude account");

        assert_eq!(loaded.accounts.len(), 1);
        assert_eq!(
            loaded.active_account_id_for_provider(Provider::Claude),
            Some(stale.id.as_str())
        );
        match &loaded.accounts[0].auth_data {
            AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                expires_at,
                subscription_type,
            } => {
                assert_eq!(access_token, "live-access");
                assert_eq!(refresh_token, "live-refresh");
                assert_eq!(*expires_at, 1_763_100_000_000);
                assert_eq!(subscription_type.as_deref(), Some("claude_max"));
            }
            other => panic!("expected ClaudeOAuth auth data, got {other:?}"),
        }
        assert_eq!(loaded.accounts[0].plan_type.as_deref(), Some("claude_max"));

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    #[ignore = "uses process-global home overrides and is flaky in full parallel suite"]
    fn load_accounts_does_not_auto_heal_when_multiple_claude_accounts_exist() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("claude-runtime-ambiguous");
        let config_dir = temp_home.join(".switchfetcher");
        let claude_dir = temp_home.join(".claude");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let first = StoredAccount::new_claude_oauth(
            "Claude A".to_string(),
            "access-a".to_string(),
            "refresh-a".to_string(),
            1_763_000_000_000,
            Some("claude_pro".to_string()),
        );
        let second = StoredAccount::new_claude_oauth(
            "Claude B".to_string(),
            "access-b".to_string(),
            "refresh-b".to_string(),
            1_763_000_000_001,
            Some("claude_max".to_string()),
        );
        let store = AccountsStore {
            version: 1,
            accounts: vec![first.clone(), second.clone()],
            active_account_id: Some(second.id.clone()),
            active_account_ids: std::collections::HashMap::from([(Provider::Claude, second.id.clone())]),
            history: Vec::new(),
        };
        save_accounts(&store).expect("seed multi-claude store");

        fs::create_dir_all(&claude_dir).expect("claude dir");
        fs::write(
            claude_dir.join(".credentials.json"),
            r#"{
                "claudeAiOauth": {
                    "accessToken": "live-access",
                    "refreshToken": "live-refresh",
                    "expiresAt": 1763100000000,
                    "subscriptionType": "claude_max"
                }
            }"#,
        )
        .expect("live claude credentials file");

        let loaded = load_accounts().expect("load should keep multi-claude state untouched");

        assert_eq!(
            loaded.active_account_id_for_provider(Provider::Claude),
            Some(second.id.as_str())
        );
        let loaded_first = loaded
            .accounts
            .iter()
            .find(|account| account.id == first.id)
            .expect("first account");
        let loaded_second = loaded
            .accounts
            .iter()
            .find(|account| account.id == second.id)
            .expect("second account");

        match &loaded_first.auth_data {
            AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                ..
            } => {
                assert_eq!(access_token, "access-a");
                assert_eq!(refresh_token, "refresh-a");
            }
            other => panic!("expected first ClaudeOAuth auth data, got {other:?}"),
        }
        match &loaded_second.auth_data {
            AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                ..
            } => {
                assert_eq!(access_token, "access-b");
                assert_eq!(refresh_token, "refresh-b");
            }
            other => panic!("expected second ClaudeOAuth auth data, got {other:?}"),
        }

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn load_accounts_marks_unrecoverable_accounts_as_broken() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("broken-partial");
        let config_dir = temp_home.join(".switchfetcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let healthy = StoredAccount::new_session_cookie(
            "Healthy Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=abc".to_string(),
        );
        let broken = StoredAccount::new_api_key(
            "Broken Codex".to_string(),
            "sk-missing".to_string(),
        );
        fs::create_dir_all(&config_dir).expect("config dir");
        let path = config_dir.join("accounts.json");
        let current = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": healthy.id,
                "name": healthy.name,
                "provider": "gemini",
                "tags": [],
                "hidden": false,
                "email": healthy.email,
                "plan_type": healthy.plan_type,
                "auth_mode": "session_cookie",
                "auth_data": healthy.auth_data,
                "secret_ref": null,
                "created_at": healthy.created_at,
                "last_used_at": healthy.last_used_at
            }, {
            "id": broken.id,
            "name": broken.name,
            "provider": "codex",
            "tags": [],
            "hidden": false,
            "email": broken.email,
            "plan_type": broken.plan_type,
            "auth_mode": "api_key",
            "auth_data": null,
            "secret_ref": "keychain:switchfetcher:missing-codex",
            "created_at": broken.created_at,
            "last_used_at": broken.last_used_at
            }],
            "active_account_id": healthy.id,
            "history": []
        });
        fs::write(&path, serde_json::to_string_pretty(&current).unwrap()).expect("accounts file");

        let report = load_accounts_report().expect("load should partially succeed");

        assert_eq!(report.store.accounts.len(), 1);
        assert_eq!(report.store.accounts[0].id, healthy.id);
        assert_eq!(report.broken_accounts.len(), 1);
        assert_eq!(report.broken_accounts[0].account.id, broken.id);

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn load_accounts_recovers_torn_accounts_file_and_writes_backup() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("torn-recovery");
        let config_dir = temp_home.join(".switchfetcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );
        fs::create_dir_all(&config_dir).expect("config dir");

        let valid_json = serde_json::json!({
            "version": 1,
            "accounts": [{
                "id": account.id,
                "name": account.name,
                "provider": "claude",
                "tags": [],
                "hidden": false,
                "email": account.email,
                "plan_type": account.plan_type,
                "auth_mode": "claude_o_auth",
                "auth_data": account.auth_data,
                "secret_ref": null,
                "created_at": account.created_at,
                "last_used_at": account.last_used_at
            }],
            "active_account_id": null,
            "history": []
        });
        let torn = format!(
            "{}{{\"id\":\"dangling-history-fragment\"}}",
            serde_json::to_string_pretty(&valid_json).expect("valid json")
        );
        let accounts_path = config_dir.join("accounts.json");
        fs::write(&accounts_path, torn).expect("torn accounts file");

        let loaded = load_accounts().expect("torn file should recover");
        let rewritten = fs::read_to_string(&accounts_path).expect("rewritten accounts file");
        let backups = fs::read_dir(&config_dir)
            .expect("config dir listing")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("accounts.json.corrupt-") && name.ends_with(".bak"))
            .collect::<Vec<_>>();

        assert_eq!(loaded.accounts.len(), 1);
        assert!(!rewritten.contains("dangling-history-fragment"));
        assert_eq!(backups.len(), 1);

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn load_accounts_syncs_active_codex_from_live_auth_file() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("codex-active-sync");
        let config_dir = temp_home.join(".switchfetcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let active = make_chatgpt_account("Active", "active@example.com", Some("chatgpt-active"));
        let stale = make_chatgpt_account("Stale", "stale@example.com", Some("chatgpt-stale"));
        let store = AccountsStore {
            version: 1,
            accounts: vec![active.clone(), stale.clone()],
            active_account_id: Some(stale.id.clone()),
            active_account_ids: std::collections::HashMap::from([(Provider::Codex, stale.id.clone())]),
            history: Vec::new(),
        };
        save_accounts(&store).expect("seed store");
        switch_to_account(&active).expect("live auth file");

        let loaded = load_accounts().expect("load should reconcile active account");

        assert_eq!(
            loaded.active_account_id_for_provider(Provider::Codex),
            Some(active.id.as_str())
        );

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn load_accounts_syncs_active_codex_from_unique_email_when_account_id_missing() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("codex-email-fallback");
        let config_dir = temp_home.join(".switchfetcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let email = "email-match@example.com";
        let stored_match = make_chatgpt_account("Stored", email, None);
        let stale = make_chatgpt_account("Stale", "stale@example.com", Some("stale-account"));
        let current_runtime = StoredAccount::new_chatgpt(
            "Runtime".to_string(),
            Some(email.to_string()),
            Some("team".to_string()),
            make_fake_jwt(email, "plus", None),
            "access-runtime".to_string(),
            "refresh-runtime".to_string(),
            None,
        );
        let store = AccountsStore {
            version: 1,
            accounts: vec![stored_match.clone(), stale.clone()],
            active_account_id: Some(stale.id.clone()),
            active_account_ids: std::collections::HashMap::from([(Provider::Codex, stale.id.clone())]),
            history: Vec::new(),
        };
        save_accounts(&store).expect("seed store");
        switch_to_account(&current_runtime).expect("runtime auth");

        let loaded = load_accounts().expect("load should reconcile from unique email");

        assert_eq!(
            loaded.active_account_id_for_provider(Provider::Codex),
            Some(stored_match.id.as_str())
        );

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_HOME");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn mark_account_switched_updates_active_last_used_and_history_together() {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp_home = make_temp_home("mark-switched");
        let config_dir = temp_home.join(".switchfetcher");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &config_dir);
            std::env::set_var("SWITCHFETCHER_SECRET_BACKEND", "file");
        }

        let active = make_chatgpt_account("Active", "active@example.com", Some("active-id"));
        let stale = make_chatgpt_account("Stale", "stale@example.com", Some("stale-id"));
        let store = AccountsStore {
            version: 1,
            accounts: vec![active.clone(), stale.clone()],
            active_account_id: Some(stale.id.clone()),
            active_account_ids: std::collections::HashMap::from([(Provider::Codex, stale.id.clone())]),
            history: Vec::new(),
        };
        save_accounts(&store).expect("seed store");

        let switched = mark_account_switched(&active.id).expect("switch mutation should succeed");
        let loaded = load_accounts().expect("updated store");

        assert_eq!(switched.id, active.id);
        assert_eq!(
            loaded.active_account_id_for_provider(Provider::Codex),
            Some(active.id.as_str())
        );
        assert!(loaded
            .accounts
            .iter()
            .find(|account| account.id == active.id)
            .and_then(|account| account.last_used_at)
            .is_some());
        assert_eq!(loaded.history.len(), 1);
        assert_eq!(loaded.history[0].kind, crate::types::AccountActionKind::Switch);

        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
            std::env::remove_var("SWITCHFETCHER_SECRET_BACKEND");
        }
        fs::remove_dir_all(temp_home).ok();
    }
}

/// Update ChatGPT OAuth tokens for an account and return the updated account.
pub fn update_account_chatgpt_tokens(
    account_id: &str,
    id_token: String,
    access_token: String,
    refresh_token: String,
    chatgpt_account_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        match &mut account.auth_data {
            AuthData::ChatGPT {
                id_token: stored_id_token,
                access_token: stored_access_token,
                refresh_token: stored_refresh_token,
                account_id: stored_account_id,
            } => {
                *stored_id_token = id_token;
                *stored_access_token = access_token;
                *stored_refresh_token = refresh_token;
                if let Some(new_account_id) = chatgpt_account_id {
                    *stored_account_id = Some(new_account_id);
                }
            }
            AuthData::ApiKey { .. } => {
                anyhow::bail!("Cannot update OAuth tokens for an API key account");
            }
            AuthData::ClaudeOAuth { .. }
            | AuthData::GeminiOAuth { .. }
            | AuthData::SessionCookie { .. } => {
                anyhow::bail!("Cannot update ChatGPT OAuth tokens for a non-ChatGPT account");
            }
        }

        if let Some(new_email) = email {
            account.email = Some(new_email);
        }

        if let Some(new_plan_type) = plan_type {
            account.plan_type = Some(new_plan_type);
        }

        Ok(account.clone())
    })
}

/// Update Claude OAuth tokens for an account and return the updated account.
pub fn update_claude_tokens(
    account_id: &str,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    subscription_type: Option<String>,
) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        match &mut account.auth_data {
            AuthData::ClaudeOAuth {
                access_token: stored_access_token,
                refresh_token: stored_refresh_token,
                expires_at: stored_expires_at,
                subscription_type: stored_subscription_type,
            } => {
                *stored_access_token = access_token;
                *stored_refresh_token = refresh_token;
                *stored_expires_at = expires_at;
                *stored_subscription_type = subscription_type.clone();
                account.plan_type = subscription_type;
            }
            AuthData::ApiKey { .. }
            | AuthData::ChatGPT { .. }
            | AuthData::GeminiOAuth { .. }
            | AuthData::SessionCookie { .. } => {
                anyhow::bail!("Cannot update Claude OAuth tokens for a non-Claude account");
            }
        }

        Ok(account.clone())
    })
}

/// Update a session cookie account.
pub fn update_session_cookie(account_id: &str, cookie: String) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        match &mut account.auth_data {
            AuthData::SessionCookie {
                cookie: stored_cookie,
            } => {
                *stored_cookie = cookie;
            }
            AuthData::ApiKey { .. }
            | AuthData::ChatGPT { .. }
            | AuthData::ClaudeOAuth { .. }
            | AuthData::GeminiOAuth { .. } => {
                anyhow::bail!("Cannot update session cookie for a non-session account");
            }
        }

        Ok(account.clone())
    })
}

/// Update Gemini OAuth tokens for an account and return the updated account.
pub fn update_gemini_tokens(
    account_id: &str,
    access_token: String,
    refresh_token: String,
    id_token: String,
    expiry_date: i64,
    email: Option<String>,
) -> Result<StoredAccount> {
    mutate_accounts_store(|store| {
        let account = store
            .accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .context("Account not found")?;

        match &mut account.auth_data {
            AuthData::GeminiOAuth {
                access_token: stored_access_token,
                refresh_token: stored_refresh_token,
                id_token: stored_id_token,
                expiry_date: stored_expiry_date,
            } => {
                *stored_access_token = access_token;
                *stored_refresh_token = refresh_token;
                *stored_id_token = id_token;
                *stored_expiry_date = expiry_date;
            }
            AuthData::ApiKey { .. }
            | AuthData::ChatGPT { .. }
            | AuthData::ClaudeOAuth { .. }
            | AuthData::SessionCookie { .. } => {
                anyhow::bail!("Cannot update Gemini OAuth tokens for a non-Gemini account");
            }
        }

        if let Some(new_email) = email {
            account.email = Some(new_email);
        }

        Ok(account.clone())
    })
}
