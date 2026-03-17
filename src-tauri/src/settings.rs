use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::auth::get_config_dir;
use crate::types::{AppSettings, NotificationPermissionState};

const SETTINGS_FILE_NAME: &str = "settings.json";

fn get_settings_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join(SETTINGS_FILE_NAME))
}

pub fn load_app_settings() -> Result<AppSettings> {
    let path = get_settings_file()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings file: {}", path.display()))?;
    let settings = serde_json::from_str::<AppSettings>(&contents)
        .with_context(|| format!("Failed to parse settings file: {}", path.display()))?;
    Ok(settings.normalized())
}

pub fn save_app_settings(settings: &AppSettings) -> Result<AppSettings> {
    let normalized = settings.clone().normalized();
    let path = get_settings_file()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create settings directory: {}", parent.display())
        })?;
    }

    fs::write(&path, serde_json::to_string_pretty(&normalized)?)
        .with_context(|| format!("Failed to write settings file: {}", path.display()))?;
    Ok(normalized)
}

fn map_permission_state(state: PermissionState) -> NotificationPermissionState {
    #[allow(unreachable_patterns)]
    match state {
        PermissionState::Granted => NotificationPermissionState::Granted,
        PermissionState::Denied => NotificationPermissionState::Denied,
        PermissionState::Prompt | PermissionState::PromptWithRationale => {
            NotificationPermissionState::Default
        }
        _ => NotificationPermissionState::Unsupported,
    }
}

#[tauri::command]
pub fn get_app_settings() -> Result<AppSettings, String> {
    load_app_settings().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn update_app_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    let saved = save_app_settings(&settings).map_err(|err| err.to_string())?;
    let _ = app.emit("settings-changed", &saved);
    Ok(saved)
}

#[tauri::command]
pub fn get_notification_permission_state(
    app: AppHandle,
) -> Result<NotificationPermissionState, String> {
    let state = app
        .notification()
        .permission_state()
        .map(map_permission_state)
        .map_err(|err| err.to_string())?;
    Ok(state)
}

#[tauri::command]
pub fn request_notification_permission(
    app: AppHandle,
) -> Result<NotificationPermissionState, String> {
    let state = app
        .notification()
        .request_permission()
        .map(map_permission_state)
        .map_err(|err| err.to_string())?;
    Ok(state)
}

#[tauri::command]
pub fn send_test_notification(app: AppHandle) -> Result<(), String> {
    let settings = load_app_settings().map_err(|err| err.to_string())?;
    if !settings.notifications_enabled {
        return Err("Notifications are disabled in settings".to_string());
    }

    let permission = app
        .notification()
        .permission_state()
        .map(map_permission_state)
        .map_err(|err| err.to_string())?;
    if permission != NotificationPermissionState::Granted {
        return Err("Notification permission is not granted".to_string());
    }

    app.notification()
        .builder()
        .title("Switchfetcher")
        .body("Test notification from Settings")
        .show()
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use uuid::Uuid;

    use super::{load_app_settings, save_app_settings};
    use crate::types::AppSettings;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn setup_temp_home() -> PathBuf {
        let base = std::env::temp_dir().join(format!("switchfetcher-settings-{}", Uuid::new_v4()));
        fs::create_dir_all(&base).expect("temp settings dir");
        unsafe {
            std::env::set_var("SWITCHFETCHER_CONFIG_DIR", &base);
        }
        base
    }

    #[test]
    fn loads_default_settings_when_file_is_missing() {
        let _guard = ENV_LOCK.lock().unwrap();
        let base = setup_temp_home();

        let settings = load_app_settings().expect("default settings should load");

        assert_eq!(settings, AppSettings::default());

        let _ = fs::remove_dir_all(base);
        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
        }
    }

    #[test]
    fn persists_normalized_settings() {
        let _guard = ENV_LOCK.lock().unwrap();
        let base = setup_temp_home();

        let saved = save_app_settings(&AppSettings {
            background_refresh_enabled: false,
            base_refresh_interval_seconds: 15,
            notifications_enabled: true,
            claude_reset_notifications_enabled: false,
            use_24h_time: false,
            usage_alert_threshold: Some(83),
        })
        .expect("settings should save");

        assert_eq!(saved.base_refresh_interval_seconds, 90);
        assert_eq!(saved.usage_alert_threshold, Some(80));
        let loaded = load_app_settings().expect("saved settings should load");
        assert_eq!(loaded, saved);

        let _ = fs::remove_dir_all(base);
        unsafe {
            std::env::remove_var("SWITCHFETCHER_CONFIG_DIR");
        }
    }
}
