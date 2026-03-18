//! Switchfetcher - Multi-provider account manager for Codex, Claude, and Gemini

pub mod api;
pub mod account_features;
pub mod auth;
pub mod commands;
pub mod settings;
pub mod types;
pub mod tray;
pub mod watcher;

use tauri::Manager;

use commands::{
    add_account_from_file, add_session_cookie_account, cancel_login, check_claude_processes,
    check_codex_processes, check_gemini_processes, complete_login, delete_account,
    delete_accounts_bulk, export_accounts_full_encrypted_file, export_accounts_slim_text,
    export_selected_accounts_full_encrypted_file, export_selected_accounts_slim_text,
    get_active_account_info, get_best_account_recommendation, get_diagnostics,
    get_provider_capabilities, get_usage, import_claude_credentials,
    import_claude_credentials_from_path, import_accounts_full_encrypted_file,
    import_accounts_slim_text, import_gemini_credentials, import_gemini_credentials_from_path,
    list_account_history, list_accounts, refresh_all_accounts_usage,
    refresh_selected_accounts_usage, rename_account, repair_account_secret, set_account_tags,
    set_provider_hidden, start_login, switch_account, warmup_account, warmup_all_accounts,
};
use settings::{
    get_app_settings, get_notification_permission_state, request_notification_permission,
    send_test_notification, update_app_settings,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(std::sync::Mutex::new(tray::TrayState::default()))
        .manage(std::sync::Mutex::new(
            commands::usage::ClaudeResetWatchState::default(),
        ))
        .manage(std::sync::Mutex::new(watcher::RefreshControllerState::default()))
        .setup(|app| {
            // Setup tray icon
            tray::create_tray(app.handle()).expect("Failed to create tray");
            watcher::spawn_background_watch(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Prevent the default close behavior
                api.prevent_close();
                // Hide the window instead of closing it
                window.hide().unwrap();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            // Account management
            list_accounts,
            get_active_account_info,
            add_account_from_file,
            import_claude_credentials,
            import_claude_credentials_from_path,
            import_gemini_credentials,
            import_gemini_credentials_from_path,
            add_session_cookie_account,
            switch_account,
            delete_account,
            delete_accounts_bulk,
            rename_account,
            set_account_tags,
            set_provider_hidden,
            export_accounts_slim_text,
            export_selected_accounts_slim_text,
            import_accounts_slim_text,
            export_accounts_full_encrypted_file,
            export_selected_accounts_full_encrypted_file,
            import_accounts_full_encrypted_file,
            list_account_history,
            get_provider_capabilities,
            get_best_account_recommendation,
            get_diagnostics,
            repair_account_secret,
            // OAuth
            start_login,
            complete_login,
            cancel_login,
            // Usage
            get_usage,
            refresh_all_accounts_usage,
            refresh_selected_accounts_usage,
            warmup_account,
            warmup_all_accounts,
            // Settings
            get_app_settings,
            update_app_settings,
            get_notification_permission_state,
            request_notification_permission,
            send_test_notification,
            // Process detection
            check_codex_processes,
            check_claude_processes,
            check_gemini_processes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
