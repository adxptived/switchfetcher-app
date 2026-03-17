use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tauri_plugin_notification::NotificationExt;

use crate::auth::{
    can_switch_account, get_account, load_accounts, mark_account_switched, switch_to_account,
};
use crate::types::{Provider, UsageInfo};
use crate::watcher;

pub struct TrayState {
    pub is_warning: bool,
    pub usage_cache: HashMap<String, UsageInfo>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            is_warning: false,
            usage_cache: HashMap::new(),
        }
    }
}

pub fn create_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_menu(app)?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();

            if id == "show_window" {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                return;
            }

            if id == "quit" {
                app.exit(0);
                return;
            }

            if let Some(account_id) = id.strip_prefix("account_") {
                let _ = switch_tray_account(app, account_id);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn switch_tray_account<R: Runtime>(
    app: &AppHandle<R>,
    account_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let account = get_account(account_id)?.ok_or("Account not found")?;
    if !can_switch_account(&account) {
        return Err("Switching is not supported for this account yet".into());
    }
    switch_to_account(&account)?;
    mark_account_switched(&account.id)?;
    notify_accounts_changed(app);
    let _ = app
        .notification()
        .builder()
        .title("Switchfetcher")
        .body(format!("Active account: {}", account.name))
        .show();
    Ok(())
}

pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>, tauri::Error> {
    let show_item = MenuItem::with_id(app, "show_window", "Open", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::new(app)?;
    menu.append(&show_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let usage_cache = {
        let state_mutex = app.state::<Mutex<TrayState>>();
        let cache = if let Ok(state) = state_mutex.lock() {
            state.usage_cache.clone()
        } else {
            HashMap::new()
        };
        cache
    };

    if let Ok(store) = load_accounts() {
        for provider in [Provider::Codex, Provider::Claude, Provider::Gemini] {
            let provider_accounts: Vec<_> = store
                .accounts
                .iter()
                .filter(|account| account.provider == provider)
                .collect();

            if provider_accounts.is_empty() {
                continue;
            }

            let header = MenuItem::with_id(
                app,
                format!("provider_{}", provider.as_str()),
                format!("── {} ──", provider.as_str().to_uppercase()),
                false,
                None::<&str>,
            )?;
            menu.append(&header)?;

            for account in provider_accounts {
                let prefix = if store.active_account_id_for_provider(account.provider)
                    == Some(account.id.as_str())
                {
                    "✓ "
                } else {
                    "  "
                };
                let usage_suffix = format_usage_for_tray(usage_cache.get(&account.id));
                let title = if can_switch_account(account) {
                    if usage_suffix.is_empty() {
                        format!("{prefix}{}", account.name)
                    } else {
                        format!("{prefix}{}  {usage_suffix}", account.name)
                    }
                } else {
                    format!("{prefix}{}  (monitor only)", account.name)
                };
                let item = MenuItem::with_id(
                    app,
                    format!("account_{}", account.id),
                    title,
                    can_switch_account(account),
                    None::<&str>,
                )?;
                menu.append(&item)?;
            }

            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
    }

    menu.append(&quit_item)?;
    Ok(menu)
}

pub fn update_tray_menu<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(build_menu(app)?))?;
    }
    Ok(())
}

pub fn notify_accounts_changed<R: Runtime>(app: &AppHandle<R>) {
    let _ = update_tray_menu(app);
    watcher::request_immediate_refresh(app);
    let _ = app.emit("accounts-changed", ());
}

pub fn update_usage_cache<R: Runtime>(app: &AppHandle<R>, usage_list: &[UsageInfo]) {
    let state_mutex = app.state::<Mutex<TrayState>>();
    if let Ok(mut state) = state_mutex.lock() {
        for usage in usage_list {
            state
                .usage_cache
                .insert(usage.account_id.clone(), usage.clone());
        }
    }
    let _ = update_tray_menu(app);
}

pub fn set_tray_icon_warning<R: Runtime>(app: &AppHandle<R>, is_warning: bool) {
    let state_mutex = app.state::<Mutex<TrayState>>();
    let mut state = match state_mutex.lock() {
        Ok(state) => state,
        Err(err) => err.into_inner(),
    };

    if state.is_warning == is_warning {
        return;
    }

    state.is_warning = is_warning;

    let Some(tray) = app.tray_by_id("main") else {
        return;
    };

    if is_warning {
        if let Ok(path) = app
            .path()
            .resolve("icons/icon-warning.png", tauri::path::BaseDirectory::Resource)
        {
            if let Ok(image) = tauri::image::Image::from_path(path) {
                let _ = tray.set_icon(Some(image));
            }
        }
    } else if let Some(default_icon) = app.default_window_icon() {
        let _ = tray.set_icon(Some(default_icon.clone()));
    }
}

fn format_usage_for_tray(usage: Option<&UsageInfo>) -> String {
    let Some(usage) = usage else {
        return String::new();
    };

    let mut parts = Vec::new();

    if let Some(percent) = usage.primary_used_percent {
        parts.push(format!("{percent:.0}% used"));
    }

    if let Some(resets_at) = usage.primary_resets_at {
        if let Some(reset_date) = DateTime::<Utc>::from_timestamp(resets_at, 0) {
            parts.push(format!("~{}", reset_date.format("%b %-d")));
        }
    }

    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::format_usage_for_tray;
    use crate::types::UsageInfo;

    fn sample_usage() -> UsageInfo {
        UsageInfo {
            account_id: "acc-1".to_string(),
            plan_type: Some("Pro".to_string()),
            primary_used_percent: Some(72.4),
            primary_window_minutes: Some(300),
            primary_resets_at: Some(1_768_608_000),
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            quota_status: None,
            daily_stats: None,
            skipped: false,
            error: None,
        }
    }

    #[test]
    fn formats_usage_percent_and_reset_for_tray() {
        let text = format_usage_for_tray(Some(&sample_usage()));

        assert_eq!(text, "72% used  ~Jan 17");
    }

    #[test]
    fn returns_empty_string_when_usage_is_missing() {
        assert!(format_usage_for_tray(None).is_empty());
    }

    #[test]
    fn formats_percent_without_reset_when_only_percent_exists() {
        let mut usage = sample_usage();
        usage.primary_resets_at = None;

        assert_eq!(format_usage_for_tray(Some(&usage)), "72% used");
    }
}
