use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, RETRY_AFTER, USER_AGENT},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::time::{sleep, Duration as TokioDuration};

use crate::auth::update_claude_tokens;
use crate::types::{AuthData, StoredAccount, UsageInfo};

const CLAUDE_CREDENTIALS_PATH: &str = ".claude/.credentials.json";
const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_SCOPE: &str = "user:profile user:inference user:sessions:claude_code";
const CLAUDE_BETA_HEADER: &str = "oauth-2025-04-20";
const CLAUDE_USER_AGENT: &str = "claude-code/1.0.0";
const CLAUDE_USAGE_MAX_ATTEMPTS: u8 = 3;
const CLAUDE_REQUEST_TIMEOUT_SECONDS: u64 = 15;
const CLAUDE_MAX_RETRY_DELAY_SECONDS: u64 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCredentials {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    #[serde(rename = "subscriptionType")]
    pub subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: ClaudeCredentials,
}

#[derive(Debug, Deserialize)]
struct ClaudeRefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    expires_at: Option<i64>,
    #[serde(default)]
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsageResponse {
    #[serde(default)]
    five_hour: Option<ClaudeUsageWindow>,
    #[serde(default)]
    seven_day: Option<ClaudeUsageWindow>,
    #[serde(default)]
    #[allow(dead_code)]
    seven_day_sonnet: Option<ClaudeUsageWindow>,
    #[serde(default)]
    #[allow(dead_code)]
    seven_day_opus: Option<ClaudeUsageWindow>,
    #[serde(default)]
    extra_usage: Option<ClaudeExtraUsage>,
    #[serde(default, alias = "tier")]
    account_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsageWindow {
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClaudeExtraUsage {
    #[serde(default)]
    is_enabled: Option<bool>,
    #[serde(default)]
    used_credits: Option<f64>,
    #[serde(default)]
    monthly_limit: Option<f64>,
}

pub async fn get_claude_usage(account: &StoredAccount) -> Result<UsageInfo> {
    let credentials = claude_credentials_from_account(account)?;

    let body = match fetch_claude_usage_body(&credentials.access_token).await {
        Ok(body) => body,
        Err(err) if is_auth_failure_error_message(&err.to_string()) => {
            refresh_usage_after_auth_failure(account, &credentials).await?
        }
        Err(err) => return Err(err),
    };

    let payload: ClaudeUsageResponse = serde_json::from_str(&body).with_context(|| {
        format!(
            "Failed to parse Claude usage response: {}",
            body.chars().take(500).collect::<String>()
        )
    })?;
    let live_subscription_type = read_claude_credentials()
        .await
        .ok()
        .and_then(|c| c.subscription_type);

    Ok(UsageInfo {
        account_id: account.id.clone(),
        plan_type: resolve_usage_plan_type(
            payload.account_type.as_deref(),
            live_subscription_type.as_deref(),
            credentials.subscription_type.as_deref(),
            account.plan_type.as_deref(),
        ),
        primary_used_percent: payload.five_hour.as_ref().and_then(|window| window.utilization),
        primary_window_minutes: payload.five_hour.as_ref().map(|_| 5 * 60),
        primary_resets_at: payload
            .five_hour
            .as_ref()
            .and_then(|window| window.resets_at.as_deref())
            .and_then(parse_reset_timestamp),
        secondary_used_percent: payload.seven_day.as_ref().and_then(|window| window.utilization),
        secondary_window_minutes: payload.seven_day.as_ref().map(|_| 7 * 24 * 60),
        secondary_resets_at: payload
            .seven_day
            .as_ref()
            .and_then(|window| window.resets_at.as_deref())
            .and_then(parse_reset_timestamp),
        has_credits: payload.extra_usage.as_ref().and_then(|extra| extra.is_enabled),
        unlimited_credits: Some(false),
        credits_balance: payload.extra_usage.as_ref().map(|extra| {
            let used = extra.used_credits.unwrap_or(0.0) / 100.0;
            let limit = extra.monthly_limit.unwrap_or(0.0) / 100.0;
            format!("${used:.2}/${limit:.2}")
        }),
        quota_status: None,
        daily_stats: None,
        skipped: false,
        error: None,
    })
}

pub async fn read_claude_credentials() -> Result<ClaudeCredentials> {
    let path = get_claude_credentials_path()?;
    read_claude_credentials_from_path(&path.to_string_lossy()).await
}

pub async fn read_claude_credentials_from_path(path: &str) -> Result<ClaudeCredentials> {
    let path = PathBuf::from(path);
    read_claude_credentials_file(&path)
}

async fn refresh_usage_after_auth_failure(
    account: &StoredAccount,
    stored_credentials: &ClaudeCredentials,
) -> Result<String> {
    let refreshed = refresh_claude_token(&stored_credentials.refresh_token)
        .await
        .context("Stored Claude OAuth credentials are invalid. Reconnect or re-import the account if needed")?;
    update_claude_tokens(
        &account.id,
        refreshed.access_token.clone(),
        refreshed.refresh_token.clone(),
        refreshed.expires_at,
        refreshed.subscription_type.clone(),
    )?;
    fetch_claude_usage_body(&refreshed.access_token).await
}

fn claude_credentials_from_account(account: &StoredAccount) -> Result<ClaudeCredentials> {
    match &account.auth_data {
        AuthData::ClaudeOAuth {
            access_token,
            refresh_token,
            expires_at,
            subscription_type,
        } => Ok(ClaudeCredentials {
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            expires_at: *expires_at,
            subscription_type: subscription_type.clone(),
        }),
        _ => anyhow::bail!("Claude account is missing OAuth credentials"),
    }
}

fn read_claude_credentials_file(path: &Path) -> Result<ClaudeCredentials> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;
    let parsed: ClaudeCredentialsFile =
        serde_json::from_str(&content).context("Failed to parse credentials file")?;
    Ok(parsed.claude_ai_oauth)
}

pub(crate) fn save_runtime_claude_credentials_to_path(
    path: &Path,
    credentials: &ClaudeCredentials,
) -> Result<()> {
    let mut root = if path.exists() {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        serde_json::from_str::<Value>(&content)
            .with_context(|| format!("Failed to parse existing Claude credentials file: {}", path.display()))?
    } else {
        Value::Object(Map::new())
    };

    let Value::Object(ref mut object) = root else {
        anyhow::bail!("Claude credentials file root must be a JSON object");
    };

    object.insert(
        "claudeAiOauth".to_string(),
        serde_json::to_value(credentials).context("Failed to serialize Claude credentials")?,
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create Claude credentials directory: {}", parent.display()))?;
    }

    let serialized = serde_json::to_string_pretty(&root)
        .context("Failed to encode Claude credentials file")?;
    std::fs::write(path, serialized)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;

    Ok(())
}

pub async fn refresh_claude_token(refresh_token: &str) -> Result<ClaudeCredentials> {
    let response = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(CLAUDE_REQUEST_TIMEOUT_SECONDS))
        .build()
        .context("Failed to create Claude OAuth client")?
        .post(CLAUDE_TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLAUDE_CLIENT_ID),
            ("scope", CLAUDE_SCOPE),
        ])
        .send()
        .await
        .context("Failed to refresh Claude OAuth token")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    ensure_success(status, &body, "Claude token refresh")?;

    let payload: ClaudeRefreshResponse =
        serde_json::from_str(&body).context("Failed to parse Claude token refresh response")?;

    Ok(ClaudeCredentials {
        access_token: payload.access_token,
        refresh_token: payload
            .refresh_token
            .unwrap_or_else(|| refresh_token.to_string()),
        expires_at: resolve_expiry_ms(payload.expires_at, payload.expires_in)?,
        subscription_type: payload.subscription_type,
    })
}

fn is_invalid_grant_response(status: StatusCode, body: &str) -> bool {
    status == StatusCode::BAD_REQUEST && body.to_ascii_lowercase().contains("invalid_grant")
}

#[cfg_attr(not(test), allow(dead_code))]
fn is_invalid_grant_error_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("invalid_grant")
}

fn is_invalid_bearer_response(status: StatusCode, body: &str) -> bool {
    status == StatusCode::UNAUTHORIZED
        && body.to_ascii_lowercase().contains("invalid bearer token")
}

fn is_invalid_bearer_error_message(message: &str) -> bool {
    message.to_ascii_lowercase().contains("invalid bearer token")
}

fn is_auth_failure_error_message(message: &str) -> bool {
    is_invalid_bearer_error_message(message)
        || message.to_ascii_lowercase().contains("authentication_error")
}

fn get_claude_credentials_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(CLAUDE_CREDENTIALS_PATH))
}

fn build_usage_headers(access_token: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(CLAUDE_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("Invalid Claude access token")?,
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(CLAUDE_BETA_HEADER),
    );
    Ok(headers)
}

async fn fetch_claude_usage_body(access_token: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(CLAUDE_REQUEST_TIMEOUT_SECONDS))
        .build()
        .context("Failed to create Claude usage client")?;
    let headers = build_usage_headers(access_token)?;

    for attempt in 1..=CLAUDE_USAGE_MAX_ATTEMPTS {
        let response = client
            .get(CLAUDE_USAGE_URL)
            .headers(headers.clone())
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let retry_after = response.headers().get(RETRY_AFTER).cloned();
                let body = response.text().await.unwrap_or_default();

                if status == StatusCode::TOO_MANY_REQUESTS {
                    if attempt == CLAUDE_USAGE_MAX_ATTEMPTS {
                        let preview = body.chars().take(240).collect::<String>();
                        anyhow::bail!(
                            "Claude usage request was rate limited after {CLAUDE_USAGE_MAX_ATTEMPTS} attempts: {preview}"
                        );
                    }

                    let delay_seconds = retry_delay_seconds(retry_after.as_ref(), attempt);
                    println!(
                        "[Claude] Usage rate limited on attempt {attempt}/{CLAUDE_USAGE_MAX_ATTEMPTS}, retrying in {delay_seconds}s"
                    );
                    sleep(TokioDuration::from_secs(delay_seconds)).await;
                    continue;
                }

                ensure_success(status, &body, "Claude usage")?;
                return Ok(body);
            }
            Err(err) => {
                if attempt == CLAUDE_USAGE_MAX_ATTEMPTS {
                    return Err(err).context("Failed to fetch Claude usage");
                }

                let delay_seconds = fallback_retry_delay_seconds(attempt);
                println!(
                    "[Claude] Usage request failed on attempt {attempt}/{CLAUDE_USAGE_MAX_ATTEMPTS}: {err}. Retrying in {delay_seconds}s"
                );
                sleep(TokioDuration::from_secs(delay_seconds)).await;
            }
        }
    }

    anyhow::bail!("Claude usage retry loop exited unexpectedly")
}

fn ensure_success(status: StatusCode, body: &str, context_name: &str) -> Result<()> {
    if status.is_success() {
        return Ok(());
    }

    let preview = body.chars().take(240).collect::<String>();
    if is_invalid_grant_response(status, body) {
        anyhow::bail!("{context_name} request failed with invalid_grant: {preview}");
    }
    if is_invalid_bearer_response(status, body) {
        anyhow::bail!("{context_name} request failed with invalid bearer token: {preview}");
    }
    anyhow::bail!("{context_name} request failed with status {status}: {preview}");
}

fn retry_delay_seconds(retry_after: Option<&HeaderValue>, attempt: u8) -> u64 {
    retry_after
        .and_then(parse_retry_after_seconds)
        .map(|seconds| seconds.min(CLAUDE_MAX_RETRY_DELAY_SECONDS))
        .unwrap_or_else(|| fallback_retry_delay_seconds(attempt))
}

fn parse_retry_after_seconds(value: &HeaderValue) -> Option<u64> {
    value.to_str().ok()?.trim().parse::<u64>().ok()
}

fn fallback_retry_delay_seconds(attempt: u8) -> u64 {
    match attempt {
        1 => 2,
        2 => 5,
        _ => 10,
    }
}

fn resolve_expiry_ms(expires_at: Option<i64>, expires_in: Option<i64>) -> Result<i64> {
    if let Some(value) = expires_at {
        return Ok(value);
    }

    if let Some(seconds) = expires_in {
        let expiry = Utc::now() + Duration::seconds(seconds);
        return Ok(expiry.timestamp_millis());
    }

    anyhow::bail!("Claude token refresh response did not include expiry information")
}

fn parse_reset_timestamp(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp())
}

fn map_account_type_to_plan_type(
    account_type: Option<&str>,
    fallback: Option<&str>,
) -> Option<String> {
    fn normalize_account_type(value: &str) -> Option<String> {
        match value.trim() {
            "" => None,
            "claude_max" | "max" => Some("Max".to_string()),
            "claude_pro" | "pro" => Some("Pro".to_string()),
            "free" => Some("Free".to_string()),
            "api_usage_billing" => Some("API".to_string()),
            _ => None,
        }
    }

    fn normalize_fallback(value: &str) -> Option<String> {
        match value.trim() {
            "" => None,
            "claude_max" | "max" => Some("Max".to_string()),
            "claude_pro" | "pro" => Some("Pro".to_string()),
            "free" => Some("Free".to_string()),
            "api_usage_billing" => Some("API".to_string()),
            other => Some(other.to_owned()),
        }
    }

    account_type
        .and_then(normalize_account_type)
        .or_else(|| fallback.and_then(normalize_fallback))
}

fn resolve_usage_plan_type(
    account_type: Option<&str>,
    live_subscription_type: Option<&str>,
    stored_subscription_type: Option<&str>,
    stored_plan_type: Option<&str>,
) -> Option<String> {
    map_account_type_to_plan_type(
        account_type,
        live_subscription_type
            .or(stored_subscription_type)
            .or(stored_plan_type),
    )
    .or_else(|| Some("claude".to_string()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        build_usage_headers, fallback_retry_delay_seconds, is_invalid_grant_error_message,
        is_invalid_grant_response, is_invalid_bearer_error_message,
        is_invalid_bearer_response, map_account_type_to_plan_type,
        resolve_usage_plan_type,
        read_claude_credentials_from_path, retry_delay_seconds,
        save_runtime_claude_credentials_to_path,
        claude_credentials_from_account, ClaudeUsageResponse,
        ClaudeCredentials,
    };
    use reqwest::{header::{HeaderValue, USER_AGENT}, StatusCode};
    use crate::types::StoredAccount;

    #[test]
    fn maps_known_account_types_to_display_plan_labels() {
        assert_eq!(
            map_account_type_to_plan_type(Some("claude_max"), None).as_deref(),
            Some("Max")
        );
        assert_eq!(
            map_account_type_to_plan_type(Some("pro"), None).as_deref(),
            Some("Pro")
        );
        assert_eq!(
            map_account_type_to_plan_type(Some("free"), None).as_deref(),
            Some("Free")
        );
        assert_eq!(
            map_account_type_to_plan_type(Some("api_usage_billing"), None).as_deref(),
            Some("API")
        );
    }

    #[test]
    fn falls_back_when_account_type_is_missing_or_unknown() {
        assert_eq!(
            map_account_type_to_plan_type(None, Some("legacy")).as_deref(),
            Some("legacy")
        );
        assert_eq!(
            map_account_type_to_plan_type(Some("unknown"), Some("legacy")).as_deref(),
            Some("legacy")
        );
    }

    #[test]
    fn normalizes_known_fallback_plan_labels() {
        assert_eq!(
            map_account_type_to_plan_type(None, Some("claude_max")).as_deref(),
            Some("Max")
        );
        assert_eq!(
            map_account_type_to_plan_type(None, Some("claude_pro")).as_deref(),
            Some("Pro")
        );
    }

    #[test]
    fn prefers_live_subscription_type_over_stale_stored_values() {
        assert_eq!(
            resolve_usage_plan_type(
                None,
                Some("claude_pro"),
                Some("claude_max"),
                Some("claude_max"),
            )
            .as_deref(),
            Some("Pro")
        );
    }

    #[test]
    fn parses_usage_response_with_optional_fields_and_float_credits() {
        let body = r#"{
            "five_hour": { "utilization": 42.5, "resets_at": null },
            "seven_day": { "utilization": null, "resets_at": "2026-03-16T12:00:00Z" },
            "seven_day_sonnet": { "utilization": 77.7, "resets_at": null },
            "seven_day_opus": { "utilization": 12.3, "resets_at": "2026-03-17T00:00:00Z" },
            "extra_usage": {
                "is_enabled": true,
                "used_credits": 1234.0,
                "monthly_limit": 5000.0
            },
            "account_type": "claude_max"
        }"#;

        let parsed: ClaudeUsageResponse =
            serde_json::from_str(body).expect("response should parse");

        assert_eq!(
            parsed.five_hour.as_ref().and_then(|w| w.utilization),
            Some(42.5)
        );
        assert_eq!(
            parsed.seven_day.as_ref().and_then(|w| w.resets_at.as_deref()),
            Some("2026-03-16T12:00:00Z")
        );
        assert_eq!(
            parsed.extra_usage.as_ref().and_then(|e| e.used_credits),
            Some(1234.0)
        );
        assert!(parsed.seven_day_sonnet.is_some());
        assert!(parsed.seven_day_opus.is_some());
    }

    #[test]
    fn uses_retry_after_header_when_it_contains_seconds() {
        let header = HeaderValue::from_static("3");
        assert_eq!(retry_delay_seconds(Some(&header), 1), 3);
    }

    #[test]
    fn falls_back_when_retry_after_header_is_missing_or_invalid() {
        let invalid = HeaderValue::from_static("soon");

        assert_eq!(retry_delay_seconds(None, 1), 2);
        assert_eq!(retry_delay_seconds(Some(&invalid), 2), 5);
    }

    #[test]
    fn caps_retry_after_to_avoid_indefinite_waits() {
        let header = HeaderValue::from_static("600");
        assert_eq!(retry_delay_seconds(Some(&header), 1), 5);
    }

    #[test]
    fn uses_expected_fallback_retry_schedule() {
        assert_eq!(fallback_retry_delay_seconds(1), 2);
        assert_eq!(fallback_retry_delay_seconds(2), 5);
        assert_eq!(fallback_retry_delay_seconds(3), 10);
        assert_eq!(fallback_retry_delay_seconds(9), 10);
    }

    #[test]
    fn detects_invalid_grant_responses_and_messages() {
        let body = r#"{"error":"invalid_grant","error_description":"Refresh token not found or invalid"}"#;

        assert!(is_invalid_grant_response(StatusCode::BAD_REQUEST, body));
        assert!(is_invalid_grant_error_message(
            "Claude token refresh request failed with invalid_grant"
        ));
        assert!(!is_invalid_grant_response(StatusCode::UNAUTHORIZED, body));
    }

    #[test]
    fn detects_invalid_bearer_responses_and_messages() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"Invalid bearer token"}}"#;

        assert!(is_invalid_bearer_response(StatusCode::UNAUTHORIZED, body));
        assert!(is_invalid_bearer_error_message(
            "Claude usage request failed with invalid bearer token"
        ));
        assert!(!is_invalid_bearer_response(StatusCode::BAD_REQUEST, body));
    }

    #[test]
    fn usage_headers_match_claude_code_user_agent() {
        let headers = build_usage_headers("token").expect("headers should build");

        assert_eq!(
            headers.get(USER_AGENT).and_then(|value| value.to_str().ok()),
            Some("claude-code/1.0.0")
        );
    }

    #[tokio::test]
    async fn reads_claude_credentials_from_explicit_path() {
        let dir = std::env::temp_dir().join(format!("switchfetcher-claude-creds-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let path = dir.join(".credentials.json");
        fs::write(
            &path,
            r#"{
                "claudeAiOauth": {
                    "accessToken": "access",
                    "refreshToken": "refresh",
                    "expiresAt": 1763000000000,
                    "subscriptionType": "claude_max"
                }
            }"#,
        )
        .expect("credentials file should be written");

        let parsed = read_claude_credentials_from_path(&path.to_string_lossy())
            .await
            .expect("credentials should parse");

        assert_eq!(parsed.access_token, "access");
        assert_eq!(parsed.refresh_token, "refresh");
        assert_eq!(parsed.expires_at, 1763000000000);
        assert_eq!(parsed.subscription_type.as_deref(), Some("claude_max"));

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn credentials_are_loaded_from_stored_account_auth_data() {
        let account = StoredAccount::new_claude_oauth(
            "Claude".to_string(),
            "stored-access".to_string(),
            "stored-refresh".to_string(),
            123,
            Some("claude_pro".to_string()),
        );

        let parsed = claude_credentials_from_account(&account)
            .expect("credentials should load from stored auth data");

        assert_eq!(parsed.access_token, "stored-access");
        assert_eq!(parsed.refresh_token, "stored-refresh");
        assert_eq!(parsed.expires_at, 123);
        assert_eq!(parsed.subscription_type.as_deref(), Some("claude_pro"));
    }

    #[test]
    fn save_runtime_credentials_fails_for_invalid_existing_json() {
        let dir = std::env::temp_dir().join(format!("switchfetcher-claude-save-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let path = dir.join(".credentials.json");
        fs::write(&path, "{").expect("broken credentials file should be written");

        let result = save_runtime_claude_credentials_to_path(
            &path,
            &ClaudeCredentials {
                access_token: "access".to_string(),
                refresh_token: "refresh".to_string(),
                expires_at: 1763000000000,
                subscription_type: Some("claude_max".to_string()),
            },
        );

        assert!(result.is_err());
        let error = result.err().expect("error should exist").to_string();
        assert!(error.contains("Failed to parse existing Claude credentials file"));

        fs::remove_dir_all(dir).ok();
    }
}
