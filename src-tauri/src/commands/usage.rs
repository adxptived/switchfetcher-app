//! Usage query Tauri commands

use std::collections::HashMap;

use futures::stream::{self, StreamExt};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::time::{sleep, timeout, Duration};

use crate::api::usage::{get_account_usage, warmup_account as send_warmup};
use crate::auth::{get_account, load_accounts, push_account_action_to_store, save_accounts};
use crate::settings::load_app_settings;
use crate::tray;
use crate::types::{
    AccountActionKind, AccountsStore, AppSettings, Provider, StoredAccount, UsageInfo,
    WarmupSummary,
};
use crate::watcher::{self, RefreshOrigin};

const CLAUDE_USAGE_PACING_MS: u64 = 750;
const CONCURRENT_USAGE_REFRESH_LIMIT: usize = 4;
const CLAUDE_USAGE_TIMEOUT_SECONDS: u64 = 20;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudeResetSnapshot {
    primary_resets_at: Option<i64>,
    secondary_resets_at: Option<i64>,
}

#[derive(Default)]
pub struct ClaudeResetWatchState {
    snapshots: HashMap<String, ClaudeResetSnapshot>,
    usage_thresholds: HashMap<String, f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ClaudeRestoreChange {
    primary_restored: bool,
    secondary_restored: bool,
    next_snapshot: ClaudeResetSnapshot,
}

/// Get usage info for a specific account
#[tauri::command]
pub async fn get_usage(app: AppHandle, account_id: String) -> Result<UsageInfo, String> {
    let mut store = load_accounts().map_err(|e| e.to_string())?;
    let account = get_account(&account_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {account_id}"))?;
    let active_account_id = store
        .active_account_id_for_provider(account.provider)
        .map(str::to_string);

    let usage = fetch_usage_for_account_with_active_id(account, active_account_id.as_deref()).await;
    let settings = load_app_settings().unwrap_or_default();
    emit_usage_updated(&app, &usage);
    emit_reset_restore_notifications(&app, &store, std::slice::from_ref(&usage), &settings);
    emit_usage_threshold_notifications(&app, &store, std::slice::from_ref(&usage), &settings);
    record_refresh_history(&mut store, std::slice::from_ref(&usage));
    save_accounts(&store).map_err(|e| e.to_string())?;
    tray::update_usage_cache(&app, std::slice::from_ref(&usage));
    tray::set_tray_icon_warning(&app, crate::watcher::should_warn_tray(&store, std::slice::from_ref(&usage)));
    Ok(usage)
}

/// Refresh usage info for all accounts
#[tauri::command]
pub async fn refresh_all_accounts_usage(app: AppHandle) -> Result<Vec<UsageInfo>, String> {
    if !watcher::begin_refresh(&app, RefreshOrigin::Manual) {
        return Err("Refresh already in progress".to_string());
    }

    let result = refresh_accounts_usage_internal(app.clone(), None).await;
    watcher::finish_refresh(
        &app,
        RefreshOrigin::Manual,
        result.as_ref().map(|_| ()).map_err(|_| ()),
    );
    result
}

#[tauri::command]
pub async fn refresh_selected_accounts_usage(
    app: AppHandle,
    account_ids: Vec<String>,
) -> Result<Vec<UsageInfo>, String> {
    if !watcher::begin_refresh(&app, RefreshOrigin::Manual) {
        return Err("Refresh already in progress".to_string());
    }

    let result = refresh_accounts_usage_internal(app.clone(), Some(account_ids)).await;
    watcher::finish_refresh(
        &app,
        RefreshOrigin::Manual,
        result.as_ref().map(|_| ()).map_err(|_| ()),
    );
    result
}

pub async fn refresh_all_accounts_usage_background(app: AppHandle) -> Result<Vec<UsageInfo>, String> {
    let result = refresh_accounts_usage_internal(app.clone(), None).await;
    watcher::finish_refresh(
        &app,
        RefreshOrigin::Automatic,
        result
            .as_ref()
            .map(|usage| automatic_refresh_result(usage))
            .unwrap_or(Err(())),
    );
    result
}

fn automatic_refresh_result(usage: &[UsageInfo]) -> Result<(), ()> {
    let mut non_skipped = usage.iter().filter(|entry| !entry.skipped);
    if non_skipped.clone().next().is_none() {
        return Ok(());
    }

    if non_skipped.all(|entry| entry.error.is_some()) {
        Err(())
    } else {
        Ok(())
    }
}

/// Send a minimal warm-up request for one account
#[tauri::command]
pub async fn warmup_account(account_id: String) -> Result<(), String> {
    let account = get_account(&account_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {account_id}"))?;

    send_warmup(&account).await.map_err(|e| e.to_string())
}

/// Send minimal warm-up requests for all accounts
#[tauri::command]
pub async fn warmup_all_accounts() -> Result<WarmupSummary, String> {
    let store = load_accounts().map_err(|e| e.to_string())?;
    let codex_accounts: Vec<_> = store
        .accounts
        .iter()
        .filter(|account| account.provider == Provider::Codex)
        .collect();
    let total_accounts = codex_accounts.len();
    let mut failed_account_ids = Vec::new();

    for account in codex_accounts {
        if send_warmup(account).await.is_err() {
            failed_account_ids.push(account.id.clone());
        }
    }

    let warmed_accounts = total_accounts.saturating_sub(failed_account_ids.len());
    Ok(WarmupSummary {
        total_accounts,
        warmed_accounts,
        failed_account_ids,
    })
}

fn emit_reset_restore_notifications(
    app: &AppHandle,
    store: &AccountsStore,
    usage_list: &[UsageInfo],
    settings: &AppSettings,
) {
    let state_mutex = app.state::<std::sync::Mutex<ClaudeResetWatchState>>();
    let mut state = state_mutex.lock().unwrap();

    for usage in usage_list {
        let Some(account) = store.accounts.iter().find(|account| account.id == usage.account_id) else {
            continue;
        };

        if !supports_reset_notifications(account.provider) {
            continue;
        }

        let change =
            detect_claude_limit_restore(state.snapshots.get(&account.id), usage);
        state
            .snapshots
            .insert(account.id.clone(), change.next_snapshot.clone());

        if should_send_reset_notification(account.provider, settings)
            && store.active_account_id_for_provider(account.provider) == Some(account.id.as_str())
            && (change.primary_restored || change.secondary_restored)
        {
            let body = build_restore_notification_body(&account.name, account.provider, &change);
            let _ = app
                .notification()
                .builder()
                .title("Switchfetcher")
                .body(body)
                .show();
        }
    }

    state.snapshots.retain(|account_id, _| {
        store.accounts.iter().any(|account| {
            supports_reset_notifications(account.provider) && account.id == *account_id
        })
    });
    state
        .usage_thresholds
        .retain(|account_id, _| store.accounts.iter().any(|account| account.id == *account_id));
}

fn emit_usage_threshold_notifications(
    app: &AppHandle,
    store: &AccountsStore,
    usage_list: &[UsageInfo],
    settings: &AppSettings,
) {
    let Some(threshold) = settings.usage_alert_threshold else {
        return;
    };
    if !settings.notifications_enabled {
        return;
    }

    let state_mutex = app.state::<std::sync::Mutex<ClaudeResetWatchState>>();
    let mut state = state_mutex.lock().unwrap();

    for usage in usage_list {
        let Some(account) = store.accounts.iter().find(|account| account.id == usage.account_id) else {
            continue;
        };
        let Some(current_percent) = usage.primary_used_percent else {
            continue;
        };
        if usage.skipped || usage.error.is_some() {
            continue;
        }

        let previous_percent = state.usage_thresholds.get(&account.id).copied();
        state
            .usage_thresholds
            .insert(account.id.clone(), current_percent);

        if store.active_account_id_for_provider(account.provider) != Some(account.id.as_str()) {
            continue;
        }

        if should_send_usage_threshold_notification(previous_percent, current_percent, threshold) {
            let _ = app
                .notification()
                .builder()
                .title("Switchfetcher")
                .body(build_usage_threshold_notification_body(&account.name, current_percent, threshold))
                .show();
        }
    }
}

fn should_send_usage_threshold_notification(
    previous_percent: Option<f64>,
    current_percent: f64,
    threshold: u8,
) -> bool {
    previous_percent.is_some_and(|previous| previous < threshold as f64)
        && current_percent >= threshold as f64
}

fn build_usage_threshold_notification_body(
    account_name: &str,
    current_percent: f64,
    threshold: u8,
) -> String {
    format!(
        "{account_name}: usage reached {:.0}% (threshold {threshold}%)",
        current_percent
    )
}

fn supports_reset_notifications(provider: Provider) -> bool {
    matches!(provider, Provider::Codex | Provider::Claude)
}

fn should_send_reset_notification(provider: Provider, settings: &AppSettings) -> bool {
    settings.notifications_enabled
        && match provider {
            Provider::Claude => settings.claude_reset_notifications_enabled,
            Provider::Codex => true,
            Provider::Gemini => false,
        }
}

fn emit_usage_updated(app: &AppHandle, usage: &UsageInfo) {
    let _ = app.emit("usage-updated", usage);
}

async fn fetch_usage_for_account_with_active_id(
    account: StoredAccount,
    active_account_id: Option<&str>,
) -> UsageInfo {
    if should_skip_usage_refresh(&account, active_account_id) {
        return UsageInfo::skipped(
            account.id.clone(),
            "Switch to this Claude account to refresh usage".to_string(),
        );
    }

    let usage_result = if account.provider == Provider::Claude {
        match timeout(
            Duration::from_secs(CLAUDE_USAGE_TIMEOUT_SECONDS),
            get_account_usage(&account),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                return UsageInfo::error(
                    account.id.clone(),
                    format!(
                        "Claude usage request timed out after {CLAUDE_USAGE_TIMEOUT_SECONDS}s"
                    ),
                );
            }
        }
    } else {
        get_account_usage(&account).await
    };

    match usage_result {
        Ok(info) => info,
        Err(err) => {
            println!("[Usage] Error for {}: {}", account.name, err);
            UsageInfo::error(account.id.clone(), err.to_string())
        }
    }
}

fn should_skip_usage_refresh(account: &StoredAccount, active_account_id: Option<&str>) -> bool {
    account.provider == Provider::Claude && active_account_id != Some(account.id.as_str())
}

async fn refresh_usage_stream(app: &AppHandle, accounts: Vec<StoredAccount>) -> Vec<UsageInfo> {
    let mut results = Vec::with_capacity(accounts.len());
    let (claude_accounts, other_accounts): (Vec<_>, Vec<_>) = accounts
        .into_iter()
        .partition(|account| account.provider == Provider::Claude);

    let mut concurrent = stream::iter(other_accounts.into_iter().map(|account| async move {
        let active_account_id = load_accounts()
            .ok()
            .and_then(|store| {
                store
                    .active_account_id_for_provider(account.provider)
                    .map(str::to_string)
            });
        fetch_usage_for_account_with_active_id(account, active_account_id.as_deref()).await
    }))
        .buffer_unordered(CONCURRENT_USAGE_REFRESH_LIMIT);
    while let Some(usage) = concurrent.next().await {
        emit_usage_updated(app, &usage);
        results.push(usage);
    }

    let mut previous_was_claude = false;
    for account in claude_accounts {
        if previous_was_claude {
            sleep(Duration::from_millis(CLAUDE_USAGE_PACING_MS)).await;
        }
        let active_account_id = load_accounts()
            .ok()
            .and_then(|store| {
                store
                    .active_account_id_for_provider(account.provider)
                    .map(str::to_string)
            });
        let usage = fetch_usage_for_account_with_active_id(account, active_account_id.as_deref()).await;
        emit_usage_updated(app, &usage);
        results.push(usage);
        previous_was_claude = true;
    }

    results
}

async fn refresh_accounts_usage_internal(
    app: AppHandle,
    selected_account_ids: Option<Vec<String>>,
) -> Result<Vec<UsageInfo>, String> {
    let mut store = load_accounts().map_err(|err| err.to_string())?;
    let accounts = match selected_account_ids {
        Some(account_ids) => {
            let requested: std::collections::HashSet<&str> =
                account_ids.iter().map(String::as_str).collect();
            store
                .accounts
                .iter()
                .filter(|account| requested.contains(account.id.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        }
        None => store.accounts.clone(),
    };
    let settings = load_app_settings().unwrap_or_default();
    let usage = refresh_usage_stream(&app, accounts).await;
    emit_reset_restore_notifications(&app, &store, &usage, &settings);
    emit_usage_threshold_notifications(&app, &store, &usage, &settings);
    record_refresh_history(&mut store, &usage);
    save_accounts(&store).map_err(|err| err.to_string())?;
    tray::update_usage_cache(&app, &usage);
    tray::set_tray_icon_warning(&app, crate::watcher::should_warn_tray(&store, &usage));
    Ok(usage)
}

fn record_refresh_history(store: &mut AccountsStore, usage_list: &[UsageInfo]) {
    for usage in usage_list {
        let Some(account) = store.accounts.iter().find(|account| account.id == usage.account_id) else {
            continue;
        };

        let last_refresh_event = store.history.iter().rev().find(|action| {
            action.account_id.as_deref() == Some(account.id.as_str())
                && matches!(
                    action.kind,
                    AccountActionKind::RefreshError | AccountActionKind::RefreshRecovered
                )
        });

        let was_error = last_refresh_event.is_some_and(|action| action.kind == AccountActionKind::RefreshError);
        if usage.skipped {
            continue;
        }
        let is_error = usage.error.is_some();
        let error_changed = is_error
            && last_refresh_event
                .and_then(|action| action.detail.as_deref())
                != usage.error.as_deref();

        match (was_error, is_error) {
            (false, true) => push_account_action_to_store(
                store,
                Some(account.id.clone()),
                Some(account.provider),
                AccountActionKind::RefreshError,
                format!("Usage refresh failed for {}", account.name),
                usage.error.clone(),
                true,
            ),
            (true, false) => push_account_action_to_store(
                store,
                Some(account.id.clone()),
                Some(account.provider),
                AccountActionKind::RefreshRecovered,
                format!("Usage refresh recovered for {}", account.name),
                None,
                false,
            ),
            (true, true) if error_changed => push_account_action_to_store(
                store,
                Some(account.id.clone()),
                Some(account.provider),
                AccountActionKind::RefreshError,
                format!("Usage refresh failed for {}", account.name),
                usage.error.clone(),
                true,
            ),
            _ => {}
        }
    }
}

fn detect_claude_limit_restore(
    previous: Option<&ClaudeResetSnapshot>,
    usage: &UsageInfo,
) -> ClaudeRestoreChange {
    let fallback_snapshot = previous.cloned().unwrap_or_default();
    if usage.error.is_some() || usage.skipped {
        return ClaudeRestoreChange {
            next_snapshot: fallback_snapshot,
            ..ClaudeRestoreChange::default()
        };
    }

    let next_snapshot = ClaudeResetSnapshot {
        primary_resets_at: usage.primary_resets_at,
        secondary_resets_at: usage.secondary_resets_at,
    };

    let Some(previous) = previous else {
        return ClaudeRestoreChange {
            next_snapshot,
            ..ClaudeRestoreChange::default()
        };
    };

    ClaudeRestoreChange {
        primary_restored: did_window_restore(
            previous.primary_resets_at,
            usage.primary_resets_at,
            usage.primary_window_minutes,
        ),
        secondary_restored: did_window_restore(
            previous.secondary_resets_at,
            usage.secondary_resets_at,
            usage.secondary_window_minutes,
        ),
        next_snapshot,
    }
}

fn did_window_restore(
    previous_resets_at: Option<i64>,
    current_resets_at: Option<i64>,
    window_minutes: Option<i64>,
) -> bool {
    let (Some(previous), Some(current)) = (previous_resets_at, current_resets_at) else {
        return false;
    };

    if current <= previous {
        return false;
    }

    (current - previous) >= restoration_jump_threshold_seconds(window_minutes)
}

fn restoration_jump_threshold_seconds(window_minutes: Option<i64>) -> i64 {
    window_minutes
        .map(|minutes| ((minutes * 60) / 2).max(60))
        .unwrap_or(60)
}

fn build_restore_notification_body(
    account_name: &str,
    provider: Provider,
    change: &ClaudeRestoreChange,
) -> String {
    let provider_label = match provider {
        Provider::Codex => "Codex",
        Provider::Claude => "Claude",
        Provider::Gemini => "Gemini",
    };
    let restored = match (change.primary_restored, change.secondary_restored, provider) {
        (true, true, Provider::Claude) => "5h and Weekly limits restored",
        (true, false, Provider::Claude) => "5h limit restored",
        (false, true, Provider::Claude) => "Weekly limit restored",
        (true, true, _) => "usage windows reset",
        (true, false, _) => "usage window reset",
        (false, true, _) => "secondary usage window reset",
        (false, false, _) => "limits updated",
    };

    format!("{account_name} ({provider_label}): {restored}")
}

#[cfg(test)]
mod tests {
    use super::{
        automatic_refresh_result, build_usage_threshold_notification_body,
        build_restore_notification_body, detect_claude_limit_restore,
        record_refresh_history, should_send_reset_notification,
        should_send_usage_threshold_notification, should_skip_usage_refresh,
        ClaudeResetSnapshot, ClaudeRestoreChange, CLAUDE_USAGE_TIMEOUT_SECONDS,
    };
    use crate::types::{AccountAction, AccountActionKind, AccountsStore, AppSettings, Provider, StoredAccount, UsageInfo};
    use chrono::Utc;

    fn usage_with_resets(
        primary_resets_at: Option<i64>,
        primary_window_minutes: Option<i64>,
        secondary_resets_at: Option<i64>,
        secondary_window_minutes: Option<i64>,
    ) -> UsageInfo {
        UsageInfo {
            account_id: "acc-1".to_string(),
            plan_type: Some("Max".to_string()),
            primary_used_percent: Some(20.0),
            primary_window_minutes,
            primary_resets_at,
            secondary_used_percent: Some(30.0),
            secondary_window_minutes,
            secondary_resets_at,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            quota_status: Some("healthy".to_string()),
            daily_stats: None,
            skipped: false,
            error: None,
        }
    }

    #[test]
    fn first_snapshot_does_not_notify() {
        let usage = usage_with_resets(Some(1_000), Some(300), Some(5_000), Some(10_080));
        let change = detect_claude_limit_restore(None, &usage);

        assert!(!change.primary_restored);
        assert!(!change.secondary_restored);
        assert_eq!(
            change.next_snapshot,
            ClaudeResetSnapshot {
                primary_resets_at: Some(1_000),
                secondary_resets_at: Some(5_000),
            }
        );
    }

    #[test]
    fn detects_primary_rollover_when_reset_jumps_by_window() {
        let previous = ClaudeResetSnapshot {
            primary_resets_at: Some(1_000),
            secondary_resets_at: Some(8_000),
        };
        let usage = usage_with_resets(Some(19_500), Some(300), Some(8_000), Some(10_080));

        let change = detect_claude_limit_restore(Some(&previous), &usage);

        assert!(change.primary_restored);
        assert!(!change.secondary_restored);
    }

    #[test]
    fn detects_secondary_rollover_when_weekly_reset_advances() {
        let previous = ClaudeResetSnapshot {
            primary_resets_at: Some(1_000),
            secondary_resets_at: Some(10_000),
        };
        let usage =
            usage_with_resets(Some(1_000), Some(300), Some(400_000), Some(10_080));

        let change = detect_claude_limit_restore(Some(&previous), &usage);

        assert!(!change.primary_restored);
        assert!(change.secondary_restored);
    }

    #[test]
    fn ignores_small_forward_drifts() {
        let previous = ClaudeResetSnapshot {
            primary_resets_at: Some(1_000),
            secondary_resets_at: Some(10_000),
        };
        let usage = usage_with_resets(Some(1_030), Some(300), Some(10_020), Some(10_080));

        let change = detect_claude_limit_restore(Some(&previous), &usage);

        assert!(!change.primary_restored);
        assert!(!change.secondary_restored);
    }

    #[test]
    fn combines_primary_and_secondary_restore_states() {
        let previous = ClaudeResetSnapshot {
            primary_resets_at: Some(1_000),
            secondary_resets_at: Some(10_000),
        };
        let usage =
            usage_with_resets(Some(19_500), Some(300), Some(400_000), Some(10_080));

        let change = detect_claude_limit_restore(Some(&previous), &usage);

        assert!(change.primary_restored);
        assert!(change.secondary_restored);
    }

    #[test]
    fn builds_codex_restore_notification_body() {
        let body = build_restore_notification_body(
            "Work",
            Provider::Codex,
            &ClaudeRestoreChange {
                primary_restored: true,
                secondary_restored: false,
                next_snapshot: ClaudeResetSnapshot::default(),
            },
        );

        assert_eq!(body, "Work (Codex): usage window reset");
    }

    #[test]
    fn threshold_notification_only_fires_on_crossing() {
        assert!(!should_send_usage_threshold_notification(None, 81.0, 80));
        assert!(!should_send_usage_threshold_notification(Some(82.0), 84.0, 80));
        assert!(should_send_usage_threshold_notification(Some(79.0), 80.0, 80));
        assert!(should_send_usage_threshold_notification(Some(74.0), 81.0, 80));
    }

    #[test]
    fn builds_usage_threshold_notification_body() {
        let body = build_usage_threshold_notification_body("Work", 84.2, 80);
        assert_eq!(body, "Work: usage reached 84% (threshold 80%)");
    }

    #[test]
    fn codex_notifications_follow_global_toggle() {
        let enabled = AppSettings::default();
        let disabled = AppSettings {
            notifications_enabled: false,
            ..AppSettings::default()
        };

        assert!(should_send_reset_notification(Provider::Codex, &enabled));
        assert!(!should_send_reset_notification(Provider::Codex, &disabled));
    }

    #[test]
    fn records_new_refresh_error_when_error_message_changes() {
        let account = StoredAccount::new_api_key("pro".to_string(), "sk-test".to_string());
        let mut store = AccountsStore::default();
        store.accounts.push(account.clone());
        store.history.push(AccountAction {
            id: "first".to_string(),
            account_id: Some(account.id.clone()),
            provider: Some(Provider::Codex),
            kind: AccountActionKind::RefreshError,
            created_at: Utc::now(),
            summary: "Usage refresh failed for pro".to_string(),
            detail: Some("Old error".to_string()),
            is_error: true,
        });

        record_refresh_history(
            &mut store,
            &[UsageInfo::error(account.id.clone(), "New error".to_string())],
        );

        let latest = store.history.last().expect("new refresh error entry");
        assert_eq!(latest.kind, AccountActionKind::RefreshError);
        assert_eq!(latest.detail.as_deref(), Some("New error"));
    }

    #[test]
    fn skipped_usage_refresh_does_not_create_refresh_history() {
        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );
        let mut store = AccountsStore::default();
        store.accounts.push(account.clone());

        record_refresh_history(
            &mut store,
            &[UsageInfo {
                account_id: account.id.clone(),
                plan_type: Some("Max".to_string()),
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
                error: Some("Switch to this Claude account to refresh usage".to_string()),
                skipped: true,
            }],
        );

        assert!(store.history.is_empty());
    }

    #[test]
    fn inactive_claude_accounts_are_skipped_for_usage_refresh() {
        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );

        assert!(should_skip_usage_refresh(&account, Some("different-account")));
        assert!(!should_skip_usage_refresh(&account, Some(account.id.as_str())));
    }

    #[test]
    fn claude_timeout_message_mentions_timeout_window() {
        let usage = UsageInfo::error(
            "acc-1".to_string(),
            format!("Claude usage request timed out after {CLAUDE_USAGE_TIMEOUT_SECONDS}s"),
        );

        assert_eq!(
            usage.error.as_deref(),
            Some("Claude usage request timed out after 20s")
        );
    }

    #[test]
    fn automatic_refresh_fails_when_all_non_skipped_accounts_fail() {
        let usage = vec![
            UsageInfo::error("acc-1".to_string(), "403 forbidden".to_string()),
            UsageInfo::skipped("acc-2".to_string(), "skipped".to_string()),
            UsageInfo::error("acc-3".to_string(), "403 forbidden".to_string()),
        ];

        assert_eq!(automatic_refresh_result(&usage), Err(()));
    }

    #[test]
    fn automatic_refresh_succeeds_when_any_non_skipped_account_succeeds() {
        let usage = vec![
            UsageInfo::error("acc-1".to_string(), "403 forbidden".to_string()),
            usage_with_resets(Some(1_000), Some(300), None, None),
        ];

        assert_eq!(automatic_refresh_result(&usage), Ok(()));
    }
}
