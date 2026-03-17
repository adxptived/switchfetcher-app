//! Account switching logic - writes credentials to ~/.codex/auth.json

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;

use crate::types::{AuthData, AuthDotJson, Provider, StoredAccount, TokenData};

/// Get the official Codex home directory
pub fn get_codex_home() -> Result<PathBuf> {
    // Check for CODEX_HOME environment variable first
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        return Ok(PathBuf::from(codex_home));
    }

    let home = get_user_home_dir()?;
    Ok(home.join(".codex"))
}

/// Get the path to the official auth.json file
pub fn get_codex_auth_file() -> Result<PathBuf> {
    Ok(get_codex_home()?.join("auth.json"))
}

/// Switch to a specific account by writing its credentials to ~/.codex/auth.json
pub fn switch_to_account(account: &StoredAccount) -> Result<()> {
    match (&account.provider, &account.auth_data) {
        (Provider::Codex, AuthData::ApiKey { .. } | AuthData::ChatGPT { .. }) => {
            let codex_home = get_codex_home()?;
            fs::create_dir_all(&codex_home)
                .with_context(|| format!("Failed to create codex home: {}", codex_home.display()))?;

            let auth_path = codex_home.join("auth.json");
            write_json_file(&auth_path, &create_auth_json(account)?)
        }
        (
            Provider::Claude,
            AuthData::ClaudeOAuth {
                access_token,
                refresh_token,
                expires_at,
                subscription_type,
            },
        ) => {
            let path = get_claude_credentials_file()?;
            let payload = ClaudeCredentialsFile {
                claude_ai_oauth: ClaudeCredentialsPayload {
                    access_token: access_token.clone(),
                    refresh_token: refresh_token.clone(),
                    expires_at: *expires_at,
                    subscription_type: subscription_type.clone(),
                },
            };
            write_json_file(&path, &payload)
        }
        (Provider::Gemini, AuthData::GeminiOAuth { .. }) => {
            anyhow::bail!("Gemini OAuth switching is not validated yet and remains disabled")
        }
        (Provider::Gemini, AuthData::SessionCookie { .. }) => {
            anyhow::bail!("Gemini session-cookie accounts are not switchable")
        }
        _ => anyhow::bail!("This account is not switchable"),
    }
}

pub fn can_switch_account(account: &StoredAccount) -> bool {
    matches!(
        (&account.provider, &account.auth_data),
        (Provider::Codex, AuthData::ApiKey { .. } | AuthData::ChatGPT { .. })
            | (Provider::Claude, AuthData::ClaudeOAuth { .. })
    )
}

/// Create an AuthDotJson structure from a StoredAccount
fn create_auth_json(account: &StoredAccount) -> Result<AuthDotJson> {
    match &account.auth_data {
        AuthData::ApiKey { key } => Ok(AuthDotJson {
            openai_api_key: Some(key.clone()),
            tokens: None,
            last_refresh: None,
        }),
        AuthData::ChatGPT {
            id_token,
            access_token,
            refresh_token,
            account_id,
        } => Ok(AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: id_token.clone(),
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                account_id: account_id.clone(),
            }),
            last_refresh: Some(Utc::now()),
        }),
        AuthData::ClaudeOAuth { .. }
        | AuthData::GeminiOAuth { .. }
        | AuthData::SessionCookie { .. } => {
            anyhow::bail!("Non-Codex accounts do not map to Codex auth.json")
        }
    }
}

/// Import an account from an existing auth.json file
pub fn import_from_auth_json(path: &str, account_name: String) -> Result<StoredAccount> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read auth.json: {path}"))?;

    let auth: AuthDotJson = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse auth.json: {path}"))?;

    // Determine auth mode and create account
    if let Some(api_key) = auth.openai_api_key {
        Ok(StoredAccount::new_api_key(account_name, api_key))
    } else if let Some(tokens) = auth.tokens {
        // Try to extract email and plan from id_token
        let (email, plan_type) = parse_id_token_claims(&tokens.id_token);

        Ok(StoredAccount::new_chatgpt(
            account_name,
            email,
            plan_type,
            tokens.id_token,
            tokens.access_token,
            tokens.refresh_token,
            tokens.account_id,
        ))
    } else {
        anyhow::bail!("auth.json contains neither API key nor tokens");
    }
}

/// Parse claims from a JWT ID token (without validation)
fn parse_id_token_claims(id_token: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return (None, None);
    }

    // Decode the payload (second part)
    let payload =
        match base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1]) {
            Ok(bytes) => bytes,
            Err(_) => return (None, None),
        };

    let json: serde_json::Value = match serde_json::from_slice(&payload) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };

    let email = json.get("email").and_then(|v| v.as_str()).map(String::from);

    // Look for plan type in the OpenAI auth claims
    let plan_type = json
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_plan_type"))
        .and_then(|v| v.as_str())
        .map(String::from);

    (email, plan_type)
}

/// Read the current auth.json file if it exists
pub fn read_current_auth() -> Result<Option<AuthDotJson>> {
    let path = get_codex_auth_file()?;

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read auth.json: {}", path.display()))?;

    let auth: AuthDotJson = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse auth.json: {}", path.display()))?;

    Ok(Some(auth))
}

/// Check if there is an active Codex login
pub fn has_active_login() -> Result<bool> {
    match read_current_auth()? {
        Some(auth) => Ok(auth.openai_api_key.is_some() || auth.tokens.is_some()),
        None => Ok(false),
    }
}

#[derive(Serialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeCredentialsPayload,
}

#[derive(Serialize)]
struct ClaudeCredentialsPayload {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
    #[serde(rename = "subscriptionType", skip_serializing_if = "Option::is_none")]
    subscription_type: Option<String>,
}

fn get_claude_credentials_file() -> Result<PathBuf> {
    let home = get_user_home_dir()?;
    Ok(home.join(".claude").join(".credentials.json"))
}

fn get_user_home_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("SWITCHFETCHER_HOME") {
        return Ok(PathBuf::from(home));
    }

    dirs::home_dir().context("Could not find home directory")
}

fn write_json_file<T: Serialize>(path: &std::path::Path, payload: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let content =
        serde_json::to_string_pretty(payload).with_context(|| format!("Failed to serialize {}", path.display()))?;
    fs::write(path, content)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{LazyLock, Mutex};

    use super::{get_codex_auth_file, switch_to_account};
    use crate::types::{Provider, StoredAccount};

    static ENV_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn make_temp_home(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "switchfetcher-switcher-tests-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    #[test]
    fn switch_to_account_writes_claude_credentials_file() {
        let _guard = ENV_MUTEX.lock().expect("env mutex should lock");
        let temp_home = make_temp_home("claude");
        unsafe {
            std::env::set_var("SWITCHFETCHER_HOME", &temp_home);
            std::env::remove_var("CODEX_HOME");
        }

        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "access".to_string(),
            "refresh".to_string(),
            1_763_000_000_000,
            Some("claude_max".to_string()),
        );

        switch_to_account(&account).expect("switch should write Claude credentials");

        let contents = fs::read_to_string(temp_home.join(".claude").join(".credentials.json"))
            .expect("Claude credentials should be written");
        assert!(contents.contains("\"claudeAiOauth\""));
        assert!(contents.contains("\"accessToken\": \"access\""));
        assert!(contents.contains("\"refreshToken\": \"refresh\""));

        fs::remove_dir_all(temp_home).ok();
    }

    #[test]
    fn switch_to_account_rejects_gemini_session_cookie_accounts() {
        let account = StoredAccount::new_session_cookie(
            "Gemini".to_string(),
            Provider::Gemini,
            "__Secure-1PSID=abc; __Secure-1PSIDTS=def".to_string(),
        );

        let error =
            switch_to_account(&account).expect_err("Gemini session cookies are not switchable");

        assert!(error.to_string().contains("not switchable"));
    }

    #[test]
    fn switch_to_account_still_writes_codex_auth_json() {
        let _guard = ENV_MUTEX.lock().expect("env mutex should lock");
        let temp_home = make_temp_home("codex");
        unsafe {
            std::env::set_var("CODEX_HOME", temp_home.join(".codex"));
            std::env::remove_var("SWITCHFETCHER_HOME");
        }

        let account = StoredAccount::new_api_key("Codex".to_string(), "sk-test".to_string());

        switch_to_account(&account).expect("switch should write codex auth");

        let auth_path = get_codex_auth_file().expect("codex auth path");
        let contents = fs::read_to_string(auth_path).expect("auth.json should exist");
        assert!(contents.contains("\"OPENAI_API_KEY\": \"sk-test\""));

        fs::remove_dir_all(temp_home).ok();
    }
}
