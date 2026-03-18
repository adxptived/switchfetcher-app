use chrono::{DateTime, Duration, Utc};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration as TokioDuration, MissedTickBehavior};

use tauri::{AppHandle, Manager, Runtime};

use crate::commands::usage::refresh_all_accounts_usage_background;
use crate::settings::load_app_settings;
use crate::types::{AccountActionKind, AccountsStore, AppSettings, UsageInfo};

const WATCH_TICK_SECONDS: u64 = 1;
const AUTO_REFRESH_FAILURE_COOLDOWN_SECONDS: i64 = 180;
const AUTO_REFRESH_PERSISTENT_FAILURE_COOLDOWN_SECONDS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOrigin {
    Manual,
    Automatic,
}

#[derive(Debug)]
pub struct RefreshControllerState {
    pub in_flight: bool,
    pub immediate_requested: bool,
    pub last_automatic_finished_at: Option<DateTime<Utc>>,
    pub last_automatic_failed_at: Option<DateTime<Utc>>,
    pub consecutive_failure_count: u32,
}

impl Default for RefreshControllerState {
    fn default() -> Self {
        Self {
            in_flight: false,
            immediate_requested: true,
            last_automatic_finished_at: None,
            last_automatic_failed_at: None,
            consecutive_failure_count: 0,
        }
    }
}

pub fn spawn_background_watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = interval(TokioDuration::from_secs(WATCH_TICK_SECONDS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            let settings = load_app_settings().unwrap_or_default();
            if !should_start_automatic_refresh(&app, &settings).await {
                continue;
            }

            if !begin_refresh(&app, RefreshOrigin::Automatic).await {
                continue;
            }

            let app_handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = refresh_all_accounts_usage_background(app_handle.clone()).await;
            });
        }
    });
}

pub fn request_immediate_refresh<R: Runtime>(app: &AppHandle<R>) {
    let state_mutex = app.state::<Mutex<RefreshControllerState>>();
    if try_request_immediate_refresh(&state_mutex) {
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state_mutex = app.state::<Mutex<RefreshControllerState>>();
        mark_immediate_refresh_requested(&state_mutex).await;
    });
}

fn try_request_immediate_refresh(state_mutex: &Mutex<RefreshControllerState>) -> bool {
    match state_mutex.try_lock() {
        Ok(mut state) => {
            state.immediate_requested = true;
            true
        }
        Err(_) => false,
    }
}

async fn mark_immediate_refresh_requested(state_mutex: &Mutex<RefreshControllerState>) {
    let mut state = state_mutex.lock().await;
    state.immediate_requested = true;
}

pub async fn begin_refresh<R: Runtime>(app: &AppHandle<R>, origin: RefreshOrigin) -> bool {
    let state_mutex = app.state::<Mutex<RefreshControllerState>>();
    let mut state = state_mutex.lock().await;
    if state.in_flight {
        if origin == RefreshOrigin::Automatic {
            state.immediate_requested = true;
        }
        return false;
    }

    state.in_flight = true;
    if origin == RefreshOrigin::Automatic {
        state.immediate_requested = false;
    }
    true
}

pub async fn finish_refresh<R: Runtime>(
    app: &AppHandle<R>,
    origin: RefreshOrigin,
    result: Result<(), ()>,
) {
    let state_mutex = app.state::<Mutex<RefreshControllerState>>();
    let mut state = state_mutex.lock().await;
    state.in_flight = false;
    if origin == RefreshOrigin::Automatic {
        let now = Utc::now();
        state.last_automatic_finished_at = Some(now);
        if result.is_ok() {
            state.consecutive_failure_count = 0;
            state.last_automatic_failed_at = None;
        } else {
            state.consecutive_failure_count += 1;
            state.last_automatic_failed_at = Some(now);
        }
    }
}

fn automatic_refresh_cooldown_seconds(consecutive_failure_count: u32) -> i64 {
    if consecutive_failure_count >= 3 {
        AUTO_REFRESH_PERSISTENT_FAILURE_COOLDOWN_SECONDS
    } else {
        AUTO_REFRESH_FAILURE_COOLDOWN_SECONDS
    }
}

async fn should_start_automatic_refresh<R: Runtime>(
    app: &AppHandle<R>,
    settings: &AppSettings,
) -> bool {
    let state_mutex = app.state::<Mutex<RefreshControllerState>>();
    let state = state_mutex.lock().await;
    if state.in_flight {
        return false;
    }

    if state.immediate_requested {
        return true;
    }

    if !settings.background_refresh_enabled {
        return false;
    }

    let now = Utc::now();
    let failure_cooldown_seconds =
        automatic_refresh_cooldown_seconds(state.consecutive_failure_count);
    let interval_seconds = match state.last_automatic_failed_at {
        Some(failed_at) if (now - failed_at).num_seconds() < failure_cooldown_seconds => {
            return false;
        }
        Some(_) => failure_cooldown_seconds as u64,
        None => settings.base_refresh_interval_seconds,
    };

    match state.last_automatic_finished_at {
        None => true,
        Some(last_finished_at) => {
            (now - last_finished_at).num_seconds() >= interval_seconds as i64
        }
    }
}

pub fn should_warn_tray(store: &AccountsStore, usage_list: &[UsageInfo]) -> bool {
    let now = Utc::now();

    let has_recent_attention_event = store.history.iter().rev().any(|action| {
        matches!(
            action.kind,
            AccountActionKind::RefreshError | AccountActionKind::RefreshRecovered
        ) && (now - action.created_at) <= Duration::minutes(10)
    });

    if has_recent_attention_event {
        return true;
    }

    usage_list.iter().any(|usage| {
        usage.error.is_some()
            || usage
                .primary_resets_at
                .and_then(|resets_at| DateTime::<Utc>::from_timestamp(resets_at, 0))
                .is_some_and(|reset_at| reset_at >= now && (reset_at - now) <= Duration::minutes(10))
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};
    use tokio::task::yield_now;
    use tokio::sync::Mutex;

    use super::{
        mark_immediate_refresh_requested, try_request_immediate_refresh,
        automatic_refresh_cooldown_seconds, should_warn_tray,
        AUTO_REFRESH_FAILURE_COOLDOWN_SECONDS, RefreshControllerState,
    };
    use crate::types::{
        AccountAction, AccountActionKind, AccountsStore, Provider, StoredAccount, UsageInfo,
    };

    fn usage(account_id: &str, resets_at: Option<i64>, error: Option<&str>) -> UsageInfo {
        UsageInfo {
            account_id: account_id.to_string(),
            plan_type: Some("pro".to_string()),
            primary_used_percent: Some(80.0),
            primary_window_minutes: Some(300),
            primary_resets_at: resets_at,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            quota_status: Some("critical".to_string()),
            daily_stats: None,
            skipped: false,
            error: error.map(str::to_string),
        }
    }

    #[test]
    fn warns_when_recent_recovery_event_exists() {
        let account = StoredAccount::new_api_key("A".to_string(), "sk-a".to_string());
        let mut store = AccountsStore::default();
        store.accounts.push(account.clone());
        store.history.push(AccountAction {
            id: "event".to_string(),
            account_id: Some(account.id),
            provider: Some(Provider::Codex),
            kind: AccountActionKind::RefreshRecovered,
            created_at: Utc::now(),
            summary: "Recovered".to_string(),
            detail: None,
            is_error: false,
        });

        assert!(should_warn_tray(&store, &[]));
    }

    #[test]
    fn warns_when_reset_window_is_imminent() {
        let account = StoredAccount::new_api_key("A".to_string(), "sk-a".to_string());
        let mut store = AccountsStore::default();
        store.accounts.push(account.clone());
        let reset_at = (Utc::now() + chrono::Duration::minutes(5)).timestamp();

        assert!(should_warn_tray(&store, &[usage(&account.id, Some(reset_at), None)]));
    }

    #[test]
    fn ignores_old_events_and_distant_resets() {
        let account = StoredAccount::new_api_key("A".to_string(), "sk-a".to_string());
        let mut store = AccountsStore::default();
        store.accounts.push(account.clone());
        store.history.push(AccountAction {
            id: "event".to_string(),
            account_id: Some(account.id.clone()),
            provider: Some(Provider::Codex),
            kind: AccountActionKind::RefreshError,
            created_at: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            summary: "Old".to_string(),
            detail: None,
            is_error: true,
        });
        let reset_at = (Utc::now() + chrono::Duration::minutes(90)).timestamp();

        assert!(!should_warn_tray(&store, &[usage(&account.id, Some(reset_at), None)]));
    }

    #[test]
    fn ignores_invalid_reset_timestamps() {
        let account = StoredAccount::new_api_key("A".to_string(), "sk-a".to_string());
        let mut store = AccountsStore::default();
        store.accounts.push(account.clone());

        assert!(!should_warn_tray(&store, &[usage(&account.id, Some(i64::MAX), None)]));
    }

    #[test]
    fn refresh_failure_cooldown_constant_is_longer_than_fast_tick() {
        assert!(AUTO_REFRESH_FAILURE_COOLDOWN_SECONDS > 60);
    }

    #[test]
    fn automatic_refresh_cooldown_extends_after_three_failures() {
        assert_eq!(
            automatic_refresh_cooldown_seconds(0),
            AUTO_REFRESH_FAILURE_COOLDOWN_SECONDS
        );
        assert_eq!(
            automatic_refresh_cooldown_seconds(2),
            AUTO_REFRESH_FAILURE_COOLDOWN_SECONDS
        );
        assert_eq!(automatic_refresh_cooldown_seconds(3), 600);
        assert_eq!(automatic_refresh_cooldown_seconds(4), 600);
    }

    #[tokio::test]
    async fn try_request_immediate_refresh_reports_busy_when_locked() {
        let state_mutex = Mutex::new(RefreshControllerState::default());
        let _guard = state_mutex.lock().await;

        assert!(!try_request_immediate_refresh(&state_mutex));
    }

    #[tokio::test]
    async fn mark_immediate_refresh_requested_waits_until_lock_is_released() {
        let state_mutex = Arc::new(Mutex::new(RefreshControllerState {
            immediate_requested: false,
            ..RefreshControllerState::default()
        }));

        let guard = state_mutex.lock().await;
        let state_clone = Arc::clone(&state_mutex);
        let task = tokio::spawn(async move {
            mark_immediate_refresh_requested(state_clone.as_ref()).await;
        });

        yield_now().await;
        assert!(!task.is_finished());

        drop(guard);
        task.await.expect("immediate refresh task should complete");

        let state = state_mutex.lock().await;
        assert!(state.immediate_requested);
    }
}
