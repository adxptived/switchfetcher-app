//! Account management Tauri commands

use crate::api::claude::{
    read_claude_credentials, read_claude_credentials_from_path, refresh_claude_token,
};
use crate::api::gemini::{
    parse_gemini_id_token_claims, read_gemini_credentials, read_gemini_credentials_from_path,
};
use crate::api::usage::refresh_all_usage;
use crate::account_features::{recommend_best_account, summarize_account_history};
use crate::auth::{
    add_account, can_switch_account, create_chatgpt_account_from_refresh_token,
    get_active_account, import_from_auth_json, load_accounts, load_accounts_report,
    mark_account_switched, push_account_action, remove_account,
    repair_account_secret as repair_secret_in_store, save_accounts,
    set_active_account, set_provider_hidden as persist_provider_hidden, switch_to_account,
    update_account_tags as persist_account_tags,
};
use crate::tray;
use crate::types::{
    AccountAction, AccountActionKind, AccountInfo, AccountLoadState, AccountsStore, AuthData,
    BestAccountRecommendation, BrokenAccountDiagnostic, DiagnosticsProviderState,
    DiagnosticsSnapshot, ImportAccountsSummary, Provider, ProviderCapabilities, StoredAccount,
};

use anyhow::Context;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use futures::{stream, StreamExt};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const SLIM_EXPORT_PREFIX: &str = "sfs1.";
const LEGACY_SLIM_EXPORT_PREFIX: &str = "css1.";
const SLIM_FORMAT_VERSION: u8 = 1;
const SLIM_AUTH_API_KEY: u8 = 0;
const SLIM_AUTH_CHATGPT: u8 = 1;
const SLIM_AUTH_CLAUDE_OAUTH: u8 = 2;
const SLIM_AUTH_SESSION_COOKIE: u8 = 3;
const SLIM_AUTH_GEMINI_OAUTH: u8 = 4;

const FULL_FILE_MAGIC: &[u8; 4] = b"SWFB";
const LEGACY_FULL_FILE_MAGIC: &[u8; 4] = b"CSWF";
const FULL_FILE_VERSION: u8 = 1;
const FULL_SALT_LEN: usize = 16;
const FULL_NONCE_LEN: usize = 24;
const FULL_KDF_ITERATIONS: u32 = 210_000;
const MAX_IMPORT_JSON_BYTES: u64 = 2 * 1024 * 1024;
const MAX_IMPORT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const SLIM_IMPORT_CONCURRENCY: usize = 6;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SlimPayload {
    #[serde(rename = "v")]
    version: u8,
    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    active_name: Option<String>,
    #[serde(rename = "c")]
    accounts: Vec<SlimAccountPayload>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SlimAccountPayload {
    #[serde(rename = "n")]
    name: String,
    #[serde(rename = "t")]
    auth_type: u8,
    #[serde(rename = "k", skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    #[serde(rename = "r", skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(rename = "c", skip_serializing_if = "Option::is_none")]
    cookie: Option<String>,
    #[serde(rename = "e", skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
    #[serde(rename = "i", skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
}

fn build_account_info(account: &StoredAccount, store: &AccountsStore) -> AccountInfo {
    let (last_action, last_refresh_error) = summarize_account_history(account, &store.history);
    AccountInfo::from_stored(
        account,
        store.active_account_id_for_provider(account.provider),
        last_action,
        last_refresh_error,
    )
}

fn build_broken_account_info(
    account: &crate::auth::storage::PersistedStoredAccount,
    store: &AccountsStore,
    reason: String,
    repair_hint: Option<String>,
) -> AccountInfo {
    let last_action = store
        .history
        .iter()
        .rev()
        .find(|action| action.account_id.as_deref() == Some(account.id.as_str()))
        .map(crate::types::AccountActionSummary::from_action);
    let last_refresh_error = store
        .history
        .iter()
        .rev()
        .find(|action| {
            action.account_id.as_deref() == Some(account.id.as_str())
                && action.kind == AccountActionKind::RefreshError
        })
        .map(crate::types::AccountActionSummary::from_action);

    AccountInfo {
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
        load_state: AccountLoadState::NeedsRepair,
        unavailable_reason: Some(reason),
        repair_hint,
        is_active: store.active_account_id_for_provider(account.provider) == Some(account.id.as_str()),
        created_at: account.created_at,
        last_used_at: account.last_used_at,
    }
}

fn build_account_list(store: &AccountsStore) -> Vec<AccountInfo> {
    store.accounts.iter().map(|account| build_account_info(account, store)).collect()
}

fn build_recommendation(
    provider: Provider,
    store: &AccountsStore,
    usage_list: &[crate::types::UsageInfo],
) -> Option<BestAccountRecommendation> {
    let recommended_id = recommend_best_account(&store.accounts, usage_list, provider)?;
    let account = store.accounts.iter().find(|account| account.id == recommended_id)?;
    let usage = usage_list.iter().find(|usage| usage.account_id == recommended_id)?;
    let remaining_percent = (100.0 - usage.primary_used_percent.unwrap_or(100.0)).max(0.0);
    let resets_at = usage.primary_resets_at;
    let reset_text = resets_at
        .map(|value| format!("reset {value}"))
        .unwrap_or_else(|| "no reset window".to_string());
    Some(BestAccountRecommendation {
        provider,
        account_id: account.id.clone(),
        account_name: account.name.clone(),
        plan_type: usage.plan_type.clone().or_else(|| account.plan_type.clone()),
        score: (remaining_percent * 100.0).round() as i64,
        reason: format!("{remaining_percent:.1}% remaining, {reset_text}"),
        remaining_percent,
        resets_at,
    })
}

/// List all accounts with their info
#[tauri::command]
pub async fn list_accounts() -> Result<Vec<AccountInfo>, String> {
    let report = load_accounts_report().map_err(|e| e.to_string())?;
    let mut accounts = build_account_list(&report.store);
    accounts.extend(report.broken_accounts.iter().map(|broken| {
        build_broken_account_info(
            &broken.account,
            &report.store,
            broken.reason.clone(),
            broken.repair_hint.clone(),
        )
    }));
    Ok(accounts)
}

/// Get the currently active account
#[tauri::command]
pub async fn get_active_account_info() -> Result<Option<AccountInfo>, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;

    if let Some(active) = get_active_account().map_err(|e| e.to_string())? {
        Ok(Some(build_account_info(&active, &store)))
    } else {
        Ok(None)
    }
}

/// Add an account from an auth.json file
#[tauri::command]
pub async fn add_account_from_file(
    app: tauri::AppHandle,
    path: String,
    name: String,
) -> Result<AccountInfo, String> {
    // Import from the file
    let account = import_from_auth_json(&path, name).map_err(|e| e.to_string())?;

    // Add to storage
    let stored = add_account(account).map_err(|e| e.to_string())?;
    switch_to_account(&stored).map_err(|e| e.to_string())?;
    set_active_account(&stored.id).map_err(|e| e.to_string())?;
    push_account_action(
        Some(stored.id.clone()),
        Some(stored.provider),
        AccountActionKind::Import,
        format!("Imported {} account from auth.json", stored.name),
        Some(path),
        false,
    )
    .map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);

    Ok(build_account_info(&stored, &store))
}

/// Import a Claude account from ~/.claude/.credentials.json.
#[tauri::command]
pub async fn import_claude_credentials(
    app: tauri::AppHandle,
    name: String,
) -> Result<AccountInfo, String> {
    let credentials = read_claude_credentials()
        .await
        .map_err(|e| e.to_string())?;
    let account = StoredAccount::new_claude_oauth(
        name,
        credentials.access_token,
        credentials.refresh_token,
        credentials.expires_at,
        credentials.subscription_type,
    );
    let stored = add_account(account).map_err(|e| e.to_string())?;
    switch_to_account(&stored).map_err(|e| e.to_string())?;
    set_active_account(&stored.id).map_err(|e| e.to_string())?;
    push_account_action(
        Some(stored.id.clone()),
        Some(stored.provider),
        AccountActionKind::Import,
        format!("Imported Claude credentials for {}", stored.name),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(build_account_info(&stored, &store))
}

/// Import a Claude account from a user-specified credentials file.
#[tauri::command]
pub async fn import_claude_credentials_from_path(
    app: tauri::AppHandle,
    name: String,
    path: String,
) -> Result<AccountInfo, String> {
    let credentials = read_claude_credentials_from_path(&path)
        .await
        .map_err(|e| e.to_string())?;
    let account = StoredAccount::new_claude_oauth(
        name,
        credentials.access_token,
        credentials.refresh_token,
        credentials.expires_at,
        credentials.subscription_type,
    );
    let stored = add_account(account).map_err(|e| e.to_string())?;
    push_account_action(
        Some(stored.id.clone()),
        Some(stored.provider),
        AccountActionKind::Import,
        format!("Imported Claude credentials for {}", stored.name),
        Some(path),
        false,
    )
    .map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(build_account_info(&stored, &store))
}

/// Add a Gemini account from a browser session cookie.
#[tauri::command]
pub async fn add_session_cookie_account(
    app: tauri::AppHandle,
    name: String,
    cookie: String,
) -> Result<AccountInfo, String> {
    let account =
        StoredAccount::new_session_cookie(name, Provider::Gemini, normalize_cookie(&cookie)?);
    let stored = add_account(account).map_err(|e| e.to_string())?;
    push_account_action(
        Some(stored.id.clone()),
        Some(stored.provider),
        AccountActionKind::Import,
        format!("Imported Gemini cookie account {}", stored.name),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(build_account_info(&stored, &store))
}

/// Import a Gemini account from ~/.gemini/oauth_creds.json.
#[tauri::command]
pub async fn import_gemini_credentials(
    app: tauri::AppHandle,
    name: String,
) -> Result<AccountInfo, String> {
    let credentials = read_gemini_credentials().await.map_err(|e| e.to_string())?;
    let (email, _) = parse_gemini_id_token_claims(&credentials.id_token);
    let account = StoredAccount::new_gemini_oauth(
        name,
        email,
        credentials.access_token,
        credentials.refresh_token,
        credentials.id_token,
        credentials.expiry_date,
    );
    let stored = add_account(account).map_err(|e| e.to_string())?;
    push_account_action(
        Some(stored.id.clone()),
        Some(stored.provider),
        AccountActionKind::Import,
        format!("Imported Gemini credentials for {}", stored.name),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(build_account_info(&stored, &store))
}

/// Import a Gemini account from a user-specified OAuth credentials file.
#[tauri::command]
pub async fn import_gemini_credentials_from_path(
    app: tauri::AppHandle,
    name: String,
    path: String,
) -> Result<AccountInfo, String> {
    let credentials = read_gemini_credentials_from_path(&path)
        .await
        .map_err(|e| e.to_string())?;
    let (email, _) = parse_gemini_id_token_claims(&credentials.id_token);
    let account = StoredAccount::new_gemini_oauth(
        name,
        email,
        credentials.access_token,
        credentials.refresh_token,
        credentials.id_token,
        credentials.expiry_date,
    );
    let stored = add_account(account).map_err(|e| e.to_string())?;
    push_account_action(
        Some(stored.id.clone()),
        Some(stored.provider),
        AccountActionKind::Import,
        format!("Imported Gemini credentials for {}", stored.name),
        Some(path),
        false,
    )
    .map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(build_account_info(&stored, &store))
}

/// Switch to a different account
#[tauri::command]
pub async fn switch_account(app: tauri::AppHandle, account_id: String) -> Result<(), String> {
    let store = load_accounts().map_err(|e| e.to_string())?;

    // Find the account
    let account = store
        .accounts
        .iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| format!("Account not found: {account_id}"))?;

    if !can_switch_account(account) {
        return Err("Switching is not supported for this account yet".to_string());
    }

    switch_to_account(account).map_err(|e| e.to_string())?;
    mark_account_switched(&account_id).map_err(|e| e.to_string())?;

    if account.provider == Provider::Codex {
        // Restart Antigravity background process if it is running
        // This allows it to pick up the new authorization file seamlessly.
        if let Ok(pids) = find_antigravity_processes() {
            for pid in pids {
                #[cfg(unix)]
                {
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(pid.to_string())
                        .output();
                }
                #[cfg(windows)]
                {
                    let _ = std::process::Command::new("taskkill")
                        .args(["/F", "/PID", &pid.to_string()])
                        .output();
                }
            }
        }
    }

    tray::notify_accounts_changed(&app);
    Ok(())
}

/// Remove an account
#[tauri::command]
pub async fn delete_account(app: tauri::AppHandle, account_id: String) -> Result<(), String> {
    remove_account(&account_id).map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_accounts_bulk(
    app: tauri::AppHandle,
    account_ids: Vec<String>,
) -> Result<(), String> {
    for account_id in account_ids {
        remove_account(&account_id).map_err(|e| e.to_string())?;
    }
    tray::notify_accounts_changed(&app);
    Ok(())
}

/// Rename an account
#[tauri::command]
pub async fn rename_account(
    app: tauri::AppHandle,
    account_id: String,
    new_name: String,
) -> Result<(), String> {
    crate::auth::storage::update_account_metadata(&account_id, Some(new_name), None, None)
        .map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn set_account_tags(
    app: tauri::AppHandle,
    account_id: String,
    tags: Vec<String>,
) -> Result<AccountInfo, String> {
    let updated = persist_account_tags(&account_id, tags).map_err(|e| e.to_string())?;
    push_account_action(
        Some(updated.id.clone()),
        Some(updated.provider),
        AccountActionKind::Import,
        format!("Updated tags for {}", updated.name),
        Some(updated.tags.join(", ")),
        false,
    )
    .map_err(|e| e.to_string())?;
    let store = load_accounts().map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(build_account_info(&updated, &store))
}

#[tauri::command]
pub async fn set_provider_hidden(
    app: tauri::AppHandle,
    provider: Provider,
    hidden: bool,
) -> Result<usize, String> {
    let updated = persist_provider_hidden(provider, hidden).map_err(|e| e.to_string())?;
    push_account_action(
        None,
        Some(provider),
        AccountActionKind::Import,
        if hidden {
            format!("Hidden {provider:?} accounts")
        } else {
            format!("Unhid {provider:?} accounts")
        },
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(updated)
}

#[tauri::command]
pub async fn list_account_history(
    account_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<AccountAction>, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let history = store
        .history
        .iter()
        .rev()
        .filter(|action| {
            account_id
                .as_deref()
                .is_none_or(|id| action.account_id.as_deref() == Some(id))
        })
        .take(limit.unwrap_or(20))
        .cloned()
        .collect();
    Ok(history)
}

#[tauri::command]
pub async fn get_provider_capabilities() -> Result<Vec<ProviderCapabilities>, String> {
    Ok(vec![
        ProviderCapabilities::from_provider(Provider::Codex),
        ProviderCapabilities::from_provider(Provider::Claude),
        ProviderCapabilities::from_provider(Provider::Gemini),
    ])
}

#[tauri::command]
pub async fn get_best_account_recommendation(
    provider: Provider,
) -> Result<Option<BestAccountRecommendation>, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let accounts: Vec<StoredAccount> = store
        .accounts
        .iter()
        .filter(|account| account.provider == provider && !account.hidden)
        .cloned()
        .collect();
    let usage_list = refresh_all_usage(&accounts).await;
    Ok(build_recommendation(provider, &store, &usage_list))
}

#[tauri::command]
pub async fn export_selected_accounts_slim_text(account_ids: Vec<String>) -> Result<String, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let selected_store = select_accounts(&store, &account_ids).map_err(|e| e.to_string())?;
    let payload = encode_slim_payload_from_store(&selected_store).map_err(|e| e.to_string())?;
    push_account_action(
        None,
        None,
        AccountActionKind::Export,
        format!("Exported slim text for {} selected accounts", selected_store.accounts.len()),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    Ok(payload)
}

#[tauri::command]
pub async fn export_selected_accounts_full_encrypted_file(
    path: String,
    passphrase: String,
    account_ids: Vec<String>,
) -> Result<(), String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let selected_store = select_accounts(&store, &account_ids).map_err(|e| e.to_string())?;
    let passphrase = normalize_backup_passphrase(&passphrase).map_err(|e| e.to_string())?;
    let encrypted =
        encode_full_encrypted_store(&selected_store, &passphrase).map_err(|e| e.to_string())?;
    write_encrypted_file(&path, &encrypted).map_err(|e| e.to_string())?;
    push_account_action(
        None,
        None,
        AccountActionKind::Export,
        format!(
            "Exported encrypted backup for {} selected accounts",
            selected_store.accounts.len()
        ),
        Some(path),
        false,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_diagnostics() -> Result<DiagnosticsSnapshot, String> {
    let report = load_accounts_report().map_err(|e| e.to_string())?;
    let store = &report.store;
    let providers = [Provider::Codex, Provider::Claude, Provider::Gemini]
        .into_iter()
        .map(|provider| {
            let active = store
                .active_account_id_for_provider(provider)
                .and_then(|active_id| store.accounts.iter().find(|account| account.id == active_id));
            let capabilities = ProviderCapabilities::from_provider(provider);
            DiagnosticsProviderState {
                provider,
                credential_path: capabilities.credential_path,
                active_account_name: active.map(|account| account.name.clone()),
                active_account_id: active.map(|account| account.id.clone()),
                supports_switch: capabilities.supports_switch,
            }
        })
        .collect();

    let recent_errors = store
        .history
        .iter()
        .rev()
        .filter(|action| action.is_error)
        .take(10)
        .map(crate::types::AccountActionSummary::from_action)
        .collect();

    Ok(DiagnosticsSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        active_account_id: store.active_account_id_for_provider(Provider::Codex).map(str::to_string),
        providers,
        broken_accounts: report
            .broken_accounts
            .iter()
            .map(|broken| BrokenAccountDiagnostic {
                account_id: broken.account.id.clone(),
                name: broken.account.name.clone(),
                provider: broken.account.provider,
                reason: broken.reason.clone(),
                suggested_source: broken.repair_hint.clone(),
            })
            .collect(),
        recent_errors,
    })
}

#[tauri::command]
pub async fn repair_account_secret(account_id: String) -> Result<(), String> {
    repair_secret_in_store(&account_id).map_err(|e| e.to_string())
}

/// Export minimal account config as a compact text string.
/// For ChatGPT accounts, only refresh token is exported.
#[tauri::command]
pub async fn export_accounts_slim_text() -> Result<String, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let payload = encode_slim_payload_from_store(&store).map_err(|e| e.to_string())?;
    push_account_action(
        None,
        None,
        AccountActionKind::Export,
        format!("Exported slim text for {} accounts", store.accounts.len()),
        None,
        false,
    )
    .map_err(|e| e.to_string())?;
    Ok(payload)
}

/// Import minimal account config from a compact text string, skipping existing accounts.
#[tauri::command]
pub async fn import_accounts_slim_text(
    app: tauri::AppHandle,
    payload: String,
) -> Result<ImportAccountsSummary, String> {
    let slim_payload = decode_slim_payload(&payload).map_err(|e| format!("{e:#}"))?;
    let total_in_payload = slim_payload.accounts.len();

    let current = load_accounts().map_err(|e| e.to_string())?;
    let existing_names: HashSet<String> = current.accounts.iter().map(|a| a.name.clone()).collect();

    let imported = build_store_from_slim_payload(slim_payload, &existing_names)
        .await
        .map_err(|e| {
            format!(
                "{e:#}\nHint: Slim import needs network access to refresh ChatGPT tokens. You can use Full encrypted file import when offline."
            )
        })?;
    validate_imported_store(&imported).map_err(|e| format!("{e:#}"))?;

    let (merged, summary) = merge_accounts_store(current, imported);
    save_accounts(&merged).map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(ImportAccountsSummary {
        total_in_payload,
        imported_count: summary.imported_count,
        skipped_count: total_in_payload.saturating_sub(summary.imported_count),
    })
}

/// Export full account config as an encrypted file.
#[tauri::command]
pub async fn export_accounts_full_encrypted_file(
    path: String,
    passphrase: String,
) -> Result<(), String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let passphrase = normalize_backup_passphrase(&passphrase).map_err(|e| e.to_string())?;
    let encrypted = encode_full_encrypted_store(&store, &passphrase).map_err(|e| e.to_string())?;
    write_encrypted_file(&path, &encrypted).map_err(|e| e.to_string())?;
    push_account_action(
        None,
        None,
        AccountActionKind::Export,
        format!("Exported encrypted backup for {} accounts", store.accounts.len()),
        Some(path),
        false,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Import full account config from an encrypted file, skipping existing accounts.
#[tauri::command]
pub async fn import_accounts_full_encrypted_file(
    app: tauri::AppHandle,
    path: String,
    passphrase: String,
) -> Result<ImportAccountsSummary, String> {
    let encrypted = read_encrypted_file(&path).map_err(|e| e.to_string())?;
    let passphrase = normalize_backup_passphrase(&passphrase).map_err(|e| e.to_string())?;
    let imported =
        decode_full_encrypted_store(&encrypted, &passphrase).map_err(|e| e.to_string())?;
    validate_imported_store(&imported).map_err(|e| e.to_string())?;

    let current = load_accounts().map_err(|e| e.to_string())?;
    let (merged, summary) = merge_accounts_store(current, imported);
    save_accounts(&merged).map_err(|e| e.to_string())?;
    push_account_action(
        None,
        None,
        AccountActionKind::Import,
        format!("Imported {} accounts from encrypted backup", summary.imported_count),
        Some(path),
        false,
    )
    .map_err(|e| e.to_string())?;
    tray::notify_accounts_changed(&app);
    Ok(summary)
}

fn select_accounts(store: &AccountsStore, account_ids: &[String]) -> anyhow::Result<AccountsStore> {
    let selected_ids: HashSet<&str> = account_ids.iter().map(String::as_str).collect();
    if selected_ids.is_empty() {
        anyhow::bail!("No accounts were selected");
    }

    let accounts: Vec<StoredAccount> = store
        .accounts
        .iter()
        .filter(|account| selected_ids.contains(account.id.as_str()))
        .cloned()
        .collect();

    if accounts.is_empty() {
        anyhow::bail!("Selected accounts were not found");
    }

    let mut export_store = AccountsStore {
        version: store.version,
        accounts,
        active_account_id: None,
        active_account_ids: store
            .active_account_ids
            .iter()
            .filter(|(_, id)| selected_ids.contains(id.as_str()))
            .map(|(provider, id)| (*provider, id.clone()))
            .collect(),
        history: Vec::new(),
    };
    export_store.normalize_active_accounts();

    Ok(export_store)
}

fn normalize_cookie(cookie: &str) -> Result<String, String> {
    let trimmed = cookie.trim();
    if trimmed.is_empty() {
        return Err("Session cookie cannot be empty".to_string());
    }

    let normalized = trimmed
        .strip_prefix("Cookie:")
        .or_else(|| trimmed.strip_prefix("cookie:"))
        .unwrap_or(trimmed)
        .trim();

    if normalized.is_empty() {
        return Err("Session cookie cannot be empty".to_string());
    }

    Ok(normalized.to_string())
}

fn normalize_backup_passphrase(passphrase: &str) -> anyhow::Result<String> {
    let trimmed = passphrase.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Backup passphrase cannot be empty");
    }

    Ok(trimmed.to_string())
}

/// Find all running Antigravity codex assistant processes
fn find_antigravity_processes() -> anyhow::Result<Vec<u32>> {
    let mut pids = Vec::new();

    #[cfg(unix)]
    {
        // Use ps with custom format to get the pid and full command line
        let output = std::process::Command::new("ps")
            .args(["-eo", "pid,command"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some((pid_str, command)) = line.split_once(' ') {
                let pid_str = pid_str.trim();
                let command = command.trim();

                // Antigravity processes have a specific path format
                let is_antigravity = (command.contains(".antigravity/extensions/openai.chatgpt")
                    || command.contains(".vscode/extensions/openai.chatgpt"))
                    && (command.ends_with("codex app-server --analytics-default-enabled")
                        || command.contains("/codex app-server"));

                if is_antigravity {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        // Use tasklist on Windows
        // For Windows we might need a more precise WMI query to get command line args,
        // but for now we look for codex.exe PIDs and verify they're not ours
        let output = std::process::Command::new("tasklist")
            .creation_flags(CREATE_NO_WINDOW)
            .args(["/FI", "IMAGENAME eq codex.exe", "/FO", "CSV", "/NH"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() > 1 {
                let name = parts[0].trim_matches('"').to_lowercase();
                if name == "codex.exe" {
                    let pid_str = parts[1].trim_matches('"');
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }
    }

    Ok(pids)
}

fn encode_slim_payload_from_store(store: &AccountsStore) -> anyhow::Result<String> {
    let active_name = store.active_account_id_for_provider(Provider::Codex).and_then(|active_id| {
        store
            .accounts
            .iter()
            .find(|account| account.id == *active_id)
            .map(|account| account.name.clone())
    });

    let slim_accounts = store
        .accounts
        .iter()
        .map(|account| match &account.auth_data {
            AuthData::ApiKey { key } => SlimAccountPayload {
                name: account.name.clone(),
                auth_type: SLIM_AUTH_API_KEY,
                api_key: Some(key.clone()),
                refresh_token: None,
                cookie: None,
                expires_at: None,
                id_token: None,
                access_token: None,
            },
            AuthData::ChatGPT { refresh_token, .. } => SlimAccountPayload {
                name: account.name.clone(),
                auth_type: SLIM_AUTH_CHATGPT,
                api_key: None,
                refresh_token: Some(refresh_token.clone()),
                cookie: None,
                expires_at: None,
                id_token: None,
                access_token: None,
            },
            AuthData::ClaudeOAuth {
                refresh_token,
                expires_at,
                ..
            } => SlimAccountPayload {
                name: account.name.clone(),
                auth_type: SLIM_AUTH_CLAUDE_OAUTH,
                api_key: None,
                refresh_token: Some(refresh_token.clone()),
                cookie: None,
                expires_at: Some(*expires_at),
                id_token: None,
                access_token: None,
            },
            AuthData::GeminiOAuth {
                access_token,
                refresh_token,
                id_token,
                expiry_date,
            } => SlimAccountPayload {
                name: account.name.clone(),
                auth_type: SLIM_AUTH_GEMINI_OAUTH,
                api_key: None,
                refresh_token: Some(refresh_token.clone()),
                cookie: None,
                expires_at: Some(*expiry_date),
                id_token: Some(id_token.clone()),
                access_token: Some(access_token.clone()),
            },
            AuthData::SessionCookie { cookie } => SlimAccountPayload {
                name: account.name.clone(),
                auth_type: SLIM_AUTH_SESSION_COOKIE,
                api_key: None,
                refresh_token: None,
                cookie: Some(cookie.clone()),
                expires_at: None,
                id_token: None,
                access_token: None,
            },
        })
        .collect();

    let payload = SlimPayload {
        version: SLIM_FORMAT_VERSION,
        active_name,
        accounts: slim_accounts,
    };

    let json = serde_json::to_vec(&payload).context("Failed to serialize slim payload")?;
    let compressed = compress_bytes(&json).context("Failed to compress slim payload")?;

    Ok(format!(
        "{SLIM_EXPORT_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(compressed)
    ))
}

fn decode_slim_payload(payload: &str) -> anyhow::Result<SlimPayload> {
    let normalized: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    if normalized.is_empty() {
        anyhow::bail!("Import string is empty");
    }

    let encoded = normalized
        .strip_prefix(SLIM_EXPORT_PREFIX)
        .or_else(|| normalized.strip_prefix(LEGACY_SLIM_EXPORT_PREFIX))
        .unwrap_or(&normalized);

    let compressed = URL_SAFE_NO_PAD
        .decode(encoded)
        .context("Invalid slim import string (base64 decode failed)")?;

    let decompressed = decompress_bytes_with_limit(&compressed, MAX_IMPORT_JSON_BYTES)
        .context("Invalid slim import string (decompression failed)")?;

    let parsed: SlimPayload = serde_json::from_slice(&decompressed)
        .context("Invalid slim import string (JSON parse failed)")?;

    validate_slim_payload(&parsed)?;
    Ok(parsed)
}

fn validate_slim_payload(payload: &SlimPayload) -> anyhow::Result<()> {
    if payload.version != SLIM_FORMAT_VERSION {
        anyhow::bail!("Unsupported slim payload version: {}", payload.version);
    }

    let mut names = HashSet::new();

    for account in &payload.accounts {
        if account.name.trim().is_empty() {
            anyhow::bail!("Slim import contains an account with empty name");
        }

        if !names.insert(account.name.clone()) {
            anyhow::bail!(
                "Slim import contains duplicate account name: {}",
                account.name
            );
        }

        match account.auth_type {
            SLIM_AUTH_API_KEY => {
                if account
                    .api_key
                    .as_ref()
                    .map_or(true, |key| key.trim().is_empty())
                {
                    anyhow::bail!("API key is missing for account {}", account.name);
                }
            }
            SLIM_AUTH_CHATGPT | SLIM_AUTH_CLAUDE_OAUTH => {
                if account
                    .refresh_token
                    .as_ref()
                    .map_or(true, |token| token.trim().is_empty())
                {
                    anyhow::bail!("Refresh token is missing for account {}", account.name);
                }
            }
            SLIM_AUTH_GEMINI_OAUTH => {
                if account
                    .refresh_token
                    .as_ref()
                    .map_or(true, |token| token.trim().is_empty())
                    || account
                        .id_token
                        .as_ref()
                        .map_or(true, |token| token.trim().is_empty())
                    || account
                        .access_token
                        .as_ref()
                        .map_or(true, |token| token.trim().is_empty())
                {
                    anyhow::bail!("Gemini OAuth fields are missing for account {}", account.name);
                }
            }
            SLIM_AUTH_SESSION_COOKIE => {
                if account
                    .cookie
                    .as_ref()
                    .map_or(true, |cookie| cookie.trim().is_empty())
                {
                    anyhow::bail!("Session cookie is missing for account {}", account.name);
                }
            }
            _ => {
                anyhow::bail!(
                    "Unsupported auth type {} for account {}",
                    account.auth_type,
                    account.name
                );
            }
        }
    }

    if let Some(active_name) = &payload.active_name {
        if !names.contains(active_name) {
            anyhow::bail!("Slim import references missing active account: {active_name}");
        }
    }

    Ok(())
}

async fn build_store_from_slim_payload(
    payload: SlimPayload,
    existing_names: &HashSet<String>,
) -> anyhow::Result<AccountsStore> {
    let active_name = payload.active_name;
    let import_candidates: Vec<SlimAccountPayload> = payload
        .accounts
        .into_iter()
        .filter(|entry| !existing_names.contains(&entry.name))
        .collect();

    let accounts = restore_slim_accounts(import_candidates).await?;
    let mut restored_store = AccountsStore {
        version: 1,
        accounts,
        active_account_id: None,
        active_account_ids: HashMap::new(),
        history: Vec::new(),
    };

    if let Some(active) = active_name {
        if let Some(account_id) = restored_store
            .accounts
            .iter()
            .find(|account| account.name == active)
            .map(|account| account.id.clone())
        {
            restored_store.set_active_account_for_provider(Provider::Codex, account_id);
        }
    }

    restored_store.normalize_active_accounts();

    if restored_store.active_account_ids.is_empty() {
        restored_store.active_account_id =
            restored_store.accounts.first().map(|a| a.id.clone());
    }

    Ok(restored_store)
}

async fn restore_slim_accounts(
    entries: Vec<SlimAccountPayload>,
) -> anyhow::Result<Vec<StoredAccount>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }

    let mut restored = Vec::with_capacity(entries.len());
    let mut tasks = stream::iter(entries.into_iter().map(|entry| async move {
        let account_name = entry.name;
        let account = match entry.auth_type {
            SLIM_AUTH_API_KEY => StoredAccount::new_api_key(
                account_name.clone(),
                entry.api_key.context("API key payload is missing")?,
            ),
            SLIM_AUTH_CHATGPT => {
                let refresh_token = entry
                    .refresh_token
                    .context("Refresh token payload is missing")?;
                create_chatgpt_account_from_refresh_token(account_name.clone(), refresh_token)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to restore ChatGPT account `{account_name}` from refresh token"
                        )
                    })?
            }
            SLIM_AUTH_CLAUDE_OAUTH => {
                let refresh_token = entry
                    .refresh_token
                    .context("Refresh token payload is missing")?;
                let credentials = refresh_claude_token(&refresh_token).await.with_context(|| {
                    format!(
                        "Failed to restore Claude account `{account_name}` from refresh token"
                    )
                })?;
                StoredAccount::new_claude_oauth(
                    account_name,
                    credentials.access_token,
                    credentials.refresh_token,
                    credentials.expires_at,
                    credentials.subscription_type,
                )
            }
            SLIM_AUTH_GEMINI_OAUTH => {
                let id_token = entry.id_token.context("Gemini id_token payload is missing")?;
                let access_token = entry
                    .access_token
                    .context("Gemini access token payload is missing")?;
                let refresh_token = entry
                    .refresh_token
                    .context("Gemini refresh token payload is missing")?;
                let expiry_date = entry
                    .expires_at
                    .context("Gemini expiry date payload is missing")?;
                let (email, _) = parse_gemini_id_token_claims(&id_token);
                StoredAccount::new_gemini_oauth(
                    account_name,
                    email,
                    access_token,
                    refresh_token,
                    id_token,
                    expiry_date,
                )
            }
            SLIM_AUTH_SESSION_COOKIE => StoredAccount::new_session_cookie(
                account_name,
                Provider::Gemini,
                entry.cookie.context("Cookie payload is missing")?,
            ),
            _ => anyhow::bail!("Unsupported auth type in slim payload"),
        };
        Ok::<StoredAccount, anyhow::Error>(account)
    }))
    .buffered(SLIM_IMPORT_CONCURRENCY);

    while let Some(account_result) = tasks.next().await {
        restored.push(account_result?);
    }

    Ok(restored)
}

fn encode_full_encrypted_store(store: &AccountsStore, passphrase: &str) -> anyhow::Result<Vec<u8>> {
    let json = serde_json::to_vec(store).context("Failed to serialize account store")?;
    let compressed = compress_bytes(&json).context("Failed to compress account store")?;

    let mut salt = [0u8; FULL_SALT_LEN];
    rand::rng().fill_bytes(&mut salt);

    let mut nonce = [0u8; FULL_NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);

    let key = derive_encryption_key(passphrase, &salt);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), compressed.as_slice())
        .map_err(|_| anyhow::anyhow!("Failed to encrypt account store"))?;

    let mut out = Vec::with_capacity(4 + 1 + FULL_SALT_LEN + FULL_NONCE_LEN + ciphertext.len());
    out.extend_from_slice(FULL_FILE_MAGIC);
    out.push(FULL_FILE_VERSION);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);

    Ok(out)
}

fn decode_full_encrypted_store(
    file_bytes: &[u8],
    passphrase: &str,
) -> anyhow::Result<AccountsStore> {
    if file_bytes.len() as u64 > MAX_IMPORT_FILE_BYTES {
        anyhow::bail!("Encrypted file is too large");
    }

    let header_len = 4 + 1 + FULL_SALT_LEN + FULL_NONCE_LEN;
    if file_bytes.len() <= header_len {
        anyhow::bail!("Encrypted file is invalid or truncated");
    }

    let magic = &file_bytes[..4];
    if magic != FULL_FILE_MAGIC && magic != LEGACY_FULL_FILE_MAGIC {
        anyhow::bail!("Encrypted file header is invalid");
    }

    let version = file_bytes[4];
    if version != FULL_FILE_VERSION {
        anyhow::bail!("Unsupported encrypted file version: {version}");
    }

    let salt_start = 5;
    let nonce_start = salt_start + FULL_SALT_LEN;
    let ciphertext_start = nonce_start + FULL_NONCE_LEN;

    let salt = &file_bytes[salt_start..nonce_start];
    let nonce = &file_bytes[nonce_start..ciphertext_start];
    let ciphertext = &file_bytes[ciphertext_start..];

    let key = derive_encryption_key(passphrase, salt);
    let cipher = XChaCha20Poly1305::new((&key).into());
    let compressed = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|_| {
            anyhow::anyhow!("Failed to decrypt file (wrong passphrase or corrupted file)")
        })?;

    let json = decompress_bytes_with_limit(&compressed, MAX_IMPORT_JSON_BYTES)
        .context("Failed to decompress decrypted payload")?;

    let store: AccountsStore =
        serde_json::from_slice(&json).context("Failed to parse decrypted account payload")?;

    Ok(store)
}

fn derive_encryption_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, FULL_KDF_ITERATIONS, &mut key);
    key
}

fn compress_bytes(input: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(input)?;
    encoder.finish().context("Failed to finalize compression")
}

fn decompress_bytes_with_limit(input: &[u8], max_bytes: u64) -> anyhow::Result<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(input);
    let mut limited = decoder.by_ref().take(max_bytes + 1);
    let mut decompressed = Vec::new();
    limited.read_to_end(&mut decompressed)?;

    if decompressed.len() as u64 > max_bytes {
        anyhow::bail!("Import data is too large");
    }

    Ok(decompressed)
}

fn write_encrypted_file(path: &str, bytes: &[u8]) -> anyhow::Result<()> {
    fs::write(path, bytes).with_context(|| format!("Failed to write file: {path}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .with_context(|| format!("Failed to set file permissions: {path}"))?;
    }

    Ok(())
}

fn read_encrypted_file(path: &str) -> anyhow::Result<Vec<u8>> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to read file metadata: {path}"))?;
    if metadata.len() > MAX_IMPORT_FILE_BYTES {
        anyhow::bail!("Encrypted file is too large");
    }

    fs::read(path).with_context(|| format!("Failed to read file: {path}"))
}

fn validate_imported_store(store: &AccountsStore) -> anyhow::Result<()> {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();

    for account in &store.accounts {
        if account.id.trim().is_empty() {
            anyhow::bail!("Import contains an account with empty id");
        }
        if account.name.trim().is_empty() {
            anyhow::bail!("Import contains an account with empty name");
        }
        if !ids.insert(account.id.clone()) {
            anyhow::bail!("Import contains duplicate account id: {}", account.id);
        }
        if !names.insert(account.name.clone()) {
            anyhow::bail!("Import contains duplicate account name: {}", account.name);
        }
    }

    for active_id in store.active_account_ids.values() {
        if !ids.contains(active_id) {
            anyhow::bail!("Import references a missing active account: {active_id}");
        }
    }

    Ok(())
}

fn merge_accounts_store(
    mut current: AccountsStore,
    imported: AccountsStore,
) -> (AccountsStore, ImportAccountsSummary) {
    let imported_version = imported.version;
    let imported_active_ids = imported.active_account_ids;
    let total_in_payload = imported.accounts.len();
    let mut imported_count = 0usize;
    let mut existing_ids: HashSet<String> = current.accounts.iter().map(|a| a.id.clone()).collect();
    let mut existing_names: HashSet<String> =
        current.accounts.iter().map(|a| a.name.clone()).collect();

    for account in imported.accounts {
        if existing_ids.contains(&account.id) || existing_names.contains(&account.name) {
            continue;
        }
        existing_ids.insert(account.id.clone());
        existing_names.insert(account.name.clone());
        current.accounts.push(account);
        imported_count += 1;
    }

    current.version = current.version.max(imported_version).max(1);

    for (provider, imported_active) in imported_active_ids {
        let current_active_is_valid = current
            .active_account_id_for_provider(provider)
            .is_some_and(|id| current.accounts.iter().any(|a| a.id == id && a.provider == provider));

        if !current_active_is_valid && current.accounts.iter().any(|a| a.id == imported_active) {
            current.set_active_account_for_provider(provider, imported_active);
        }
    }

    current.normalize_active_accounts();

    (
        current,
        ImportAccountsSummary {
            total_in_payload,
            imported_count,
            skipped_count: total_in_payload.saturating_sub(imported_count),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{
        decode_full_encrypted_store, decode_slim_payload, encode_full_encrypted_store,
        encode_slim_payload_from_store, normalize_backup_passphrase, AccountsStore, AuthData,
        Provider, StoredAccount,
    };

    #[test]
    fn slim_export_uses_switchfetcher_prefix_for_claude_oauth_accounts() {
        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_pro".to_string()),
        );
        let store = AccountsStore {
            version: 1,
            active_account_id: Some(account.id.clone()),
            active_account_ids: std::collections::HashMap::from([(
                Provider::Claude,
                account.id.clone(),
            )]),
            accounts: vec![account],
            history: Vec::new(),
        };

        let encoded = encode_slim_payload_from_store(&store).expect("slim export should succeed");

        assert!(encoded.starts_with("sfs1."));

        let decoded = decode_slim_payload(&encoded).expect("slim import should succeed");
        assert_eq!(decoded.accounts.len(), 1);
        assert_eq!(decoded.accounts[0].auth_type, 2);
        assert_eq!(decoded.accounts[0].refresh_token.as_deref(), Some("refresh"));
        assert_eq!(decoded.accounts[0].expires_at, Some(1_763_000_000_000));
    }

    #[test]
    fn full_export_round_trips_gemini_session_cookie_accounts() {
        let account = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=abc; __Secure-1PSIDTS=def".to_string(),
        );
        let store = AccountsStore {
            version: 1,
            active_account_id: Some(account.id.clone()),
            active_account_ids: std::collections::HashMap::from([(
                Provider::Gemini,
                account.id.clone(),
            )]),
            accounts: vec![account],
            history: Vec::new(),
        };

        let encrypted = encode_full_encrypted_store(&store, "shared-passphrase").expect("export");
        let decoded =
            decode_full_encrypted_store(&encrypted, "shared-passphrase").expect("import");

        assert_eq!(decoded.accounts.len(), 1);
        assert_eq!(decoded.accounts[0].provider, Provider::Gemini);
        match &decoded.accounts[0].auth_data {
            AuthData::SessionCookie { cookie } => {
                assert_eq!(cookie, "__Secure-1PSID=abc; __Secure-1PSIDTS=def");
            }
            other => panic!("unexpected auth data: {other:?}"),
        }
    }

    #[test]
    fn full_export_requires_non_empty_passphrase() {
        let error = normalize_backup_passphrase("   ").expect_err("blank passphrase should fail");

        assert!(error.to_string().contains("passphrase"));
    }

    #[test]
    fn full_export_rejects_wrong_passphrase_on_import() {
        let account = StoredAccount::new_api_key("Codex".to_string(), "sk-test".to_string());
        let store = AccountsStore {
            version: 1,
            active_account_id: Some(account.id.clone()),
            active_account_ids: std::collections::HashMap::from([(
                Provider::Codex,
                account.id.clone(),
            )]),
            accounts: vec![account],
            history: Vec::new(),
        };

        let encrypted =
            encode_full_encrypted_store(&store, "correct horse battery staple").expect("export");
        let error = decode_full_encrypted_store(&encrypted, "wrong passphrase")
            .expect_err("wrong passphrase should fail");

        assert!(error.to_string().contains("wrong passphrase"));
    }
}
