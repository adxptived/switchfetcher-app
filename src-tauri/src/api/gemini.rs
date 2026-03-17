use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE, REFERER, USER_AGENT},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::update_gemini_tokens;
use crate::types::{AuthData, StoredAccount, UsageInfo};

const GEMINI_CREDS_PATH: &str = ".gemini/oauth_creds.json";
const GEMINI_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36";
const GEMINI_APP_URL: &str = "https://gemini.google.com/app";
const GEMINI_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GEMINI_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";
const GEMINI_LOAD_CODE_ASSIST_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const GEMINI_RETRIEVE_USER_QUOTA_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";
const REFRESH_SKEW_SECONDS: i64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub expiry_date: i64,
}

#[derive(Debug, Deserialize)]
struct GeminiRefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    #[serde(default)]
    email: Option<String>,
}

pub async fn get_gemini_usage(account: &StoredAccount) -> Result<UsageInfo> {
    match &account.auth_data {
        AuthData::SessionCookie { cookie } => get_gemini_usage_from_cookie(&account.id, cookie).await,
        AuthData::GeminiOAuth {
            access_token,
            refresh_token,
            id_token,
            expiry_date,
        } => {
            let mut credentials = GeminiCredentials {
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                id_token: id_token.clone(),
                expiry_date: *expiry_date,
            };

            if needs_refresh(credentials.expiry_date) {
                let refreshed = refresh_gemini_token(&credentials).await?;
                let email = fetch_user_email(&refreshed.access_token).await.ok().flatten();
                let persisted = update_gemini_tokens(
                    &account.id,
                    refreshed.access_token.clone(),
                    refreshed.refresh_token.clone(),
                    refreshed.id_token.clone(),
                    refreshed.expiry_date,
                    email,
                )?;

                credentials = match persisted.auth_data {
                    AuthData::GeminiOAuth {
                        access_token,
                        refresh_token,
                        id_token,
                        expiry_date,
                    } => GeminiCredentials {
                        access_token,
                        refresh_token,
                        id_token,
                        expiry_date,
                    },
                    _ => anyhow::bail!("Gemini account auth mode changed unexpectedly"),
                };
            }

            get_gemini_usage_from_oauth(account, &credentials).await
        }
        _ => anyhow::bail!("Gemini account is missing supported authentication"),
    }
}

pub async fn read_gemini_credentials() -> Result<GeminiCredentials> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let path = home.join(GEMINI_CREDS_PATH);
    read_gemini_credentials_from_path(&path.to_string_lossy()).await
}

pub async fn read_gemini_credentials_from_path(path: &str) -> Result<GeminiCredentials> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {path}"))?;
    serde_json::from_str(&content).context("Failed to parse Gemini credentials file")
}

pub fn parse_gemini_id_token_claims(id_token: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return (None, None);
    }

    let payload = match base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => return (None, None),
    };

    let json: Value = match serde_json::from_slice(&payload) {
        Ok(value) => value,
        Err(_) => return (None, None),
    };

    let email = json.get("email").and_then(Value::as_str).map(ToOwned::to_owned);
    let client_id = json.get("aud").and_then(Value::as_str).map(ToOwned::to_owned);
    (email, client_id)
}

async fn get_gemini_usage_from_cookie(account_id: &str, cookie: &str) -> Result<UsageInfo> {
    validate_gemini_cookie(cookie)?;

    let response = reqwest::Client::new()
        .get(GEMINI_APP_URL)
        .headers(build_gemini_cookie_headers(cookie)?)
        .send()
        .await
        .context("Failed to fetch Gemini app page")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    ensure_success(status, &body)?;

    if let Some(info) = parse_gemini_usage_response(account_id, &body)? {
        return Ok(info);
    }

    Ok(UsageInfo::error(
        account_id.to_string(),
        "Gemini quota data was not found in the current web response. Capture the live quota request from an authenticated browser session and mirror that contract in api/gemini.rs.".to_string(),
    ))
}

async fn get_gemini_usage_from_oauth(account: &StoredAccount, credentials: &GeminiCredentials) -> Result<UsageInfo> {
    let client = reqwest::Client::new();
    let headers = build_gemini_oauth_headers(&credentials.access_token)?;

    let load_response = client
        .post(GEMINI_LOAD_CODE_ASSIST_URL)
        .headers(headers.clone())
        .json(&json!({}))
        .send()
        .await
        .context("Failed to initialize Gemini OAuth quota session")?;
    let load_status = load_response.status();
    let load_body = load_response.text().await.unwrap_or_default();
    ensure_success(load_status, &load_body)?;

    let quota_response = client
        .post(GEMINI_RETRIEVE_USER_QUOTA_URL)
        .headers(headers)
        .json(&json!({}))
        .send()
        .await
        .context("Failed to fetch Gemini OAuth quota")?;
    let quota_status = quota_response.status();
    let quota_body = quota_response.text().await.unwrap_or_default();
    ensure_success(quota_status, &quota_body)?;

    if let Some(info) = parse_gemini_usage_response(&account.id, &quota_body)? {
        return Ok(info);
    }

    Ok(UsageInfo::error(
        account.id.clone(),
        format!(
            "Gemini OAuth quota response did not match known patterns: {}",
            quota_body.chars().take(500).collect::<String>()
        ),
    ))
}

pub async fn refresh_gemini_token(credentials: &GeminiCredentials) -> Result<GeminiCredentials> {
    let (_, client_id) = parse_gemini_id_token_claims(&credentials.id_token);
    let client_id = client_id.context("Gemini id_token did not contain an OAuth client ID")?;

    let response = reqwest::Client::new()
        .post(GEMINI_TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", credentials.refresh_token.as_str()),
            ("client_id", client_id.as_str()),
        ])
        .send()
        .await
        .context("Failed to refresh Gemini OAuth token")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    ensure_success(status, &body)?;

    let payload: GeminiRefreshResponse =
        serde_json::from_str(&body).context("Failed to parse Gemini token refresh response")?;

    Ok(GeminiCredentials {
        access_token: payload.access_token,
        refresh_token: payload
            .refresh_token
            .unwrap_or_else(|| credentials.refresh_token.clone()),
        id_token: payload
            .id_token
            .unwrap_or_else(|| credentials.id_token.clone()),
        expiry_date: chrono::Utc::now().timestamp_millis() + (payload.expires_in * 1000),
    })
}

async fn fetch_user_email(access_token: &str) -> Result<Option<String>> {
    let response = reqwest::Client::new()
        .get(GEMINI_USERINFO_URL)
        .headers(build_gemini_oauth_headers(access_token)?)
        .send()
        .await
        .context("Failed to fetch Gemini user info")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    ensure_success(status, &body)?;

    let user_info: GoogleUserInfo = serde_json::from_str(&body).context("Failed to parse Google user info")?;
    Ok(user_info.email)
}

fn needs_refresh(expiry_date_ms: i64) -> bool {
    expiry_date_ms <= (chrono::Utc::now().timestamp_millis() + (REFRESH_SKEW_SECONDS * 1000))
}

fn validate_gemini_cookie(cookie: &str) -> Result<()> {
    if !cookie.contains("__Secure-1PSID=") || !cookie.contains("__Secure-1PSIDTS=") {
        anyhow::bail!(
            "Gemini cookie must include both __Secure-1PSID and __Secure-1PSIDTS values"
        );
    }
    Ok(())
}

fn build_gemini_cookie_headers(cookie: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(GEMINI_USER_AGENT));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    headers.insert(REFERER, HeaderValue::from_static("https://gemini.google.com/"));
    headers.insert(
        COOKIE,
        HeaderValue::from_str(cookie).context("Invalid Gemini cookie header")?,
    );
    Ok(headers)
}

fn build_gemini_oauth_headers(access_token: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(GEMINI_USER_AGENT));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}")).context("Invalid Gemini access token")?,
    );
    Ok(headers)
}

fn ensure_success(status: StatusCode, body: &str) -> Result<()> {
    if status.is_success() {
        return Ok(());
    }

    let preview = body.chars().take(200).collect::<String>();
    anyhow::bail!("Gemini request failed with status {status}: {preview}");
}

fn parse_gemini_usage_response(account_id: &str, body: &str) -> Result<Option<UsageInfo>> {
    if let Ok(value) = serde_json::from_str::<Value>(body) {
        if let Some(info) = parse_usage_value(account_id, &value) {
            return Ok(Some(info));
        }
    }

    for line in body.lines() {
        let trimmed = line.trim();
        if !(trimmed.contains("quota")
            || trimmed.contains("limit")
            || trimmed.contains("remaining")
            || trimmed.contains("usage"))
        {
            continue;
        }

        if let Some(start) = trimmed.find('{') {
            if let Some(end) = trimmed.rfind('}') {
                if end > start {
                    let candidate = &trimmed[start..=end];
                    if let Ok(value) = serde_json::from_str::<Value>(candidate) {
                        if let Some(info) = parse_usage_value(account_id, &value) {
                            return Ok(Some(info));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

fn parse_usage_value(account_id: &str, value: &Value) -> Option<UsageInfo> {
    let usage_root = value.get("usage").unwrap_or(value);
    let mut windows = Vec::new();
    collect_windows(usage_root, &mut windows);
    if windows.is_empty() && usage_root != value {
        collect_windows(value, &mut windows);
    }
    windows.sort_by_key(|window| window.window_minutes.unwrap_or(i64::MAX));

    let primary = windows.first().cloned()?;
    let secondary = windows.get(1).cloned();
    let plan_type = extract_plan_type(usage_root)
        .or_else(|| extract_plan_type(value))
        .or_else(|| Some("gemini".to_string()));

    Some(UsageInfo {
        account_id: account_id.to_string(),
        plan_type,
        primary_used_percent: Some(primary.used_percent),
        primary_window_minutes: primary.window_minutes,
        primary_resets_at: primary.resets_at,
        secondary_used_percent: secondary.as_ref().map(|w| w.used_percent),
        secondary_window_minutes: secondary.as_ref().and_then(|w| w.window_minutes),
        secondary_resets_at: secondary.as_ref().and_then(|w| w.resets_at),
        has_credits: None,
        unlimited_credits: None,
        credits_balance: None,
        quota_status: None,
        daily_stats: None,
        skipped: false,
        error: None,
    })
}

#[derive(Clone)]
struct ParsedWindow {
    used_percent: f64,
    window_minutes: Option<i64>,
    resets_at: Option<i64>,
}

fn collect_windows(value: &Value, windows: &mut Vec<ParsedWindow>) {
    match value {
        Value::Object(map) => {
            if let Some(window) = parse_window(map) {
                windows.push(window);
            }
            for child in map.values() {
                collect_windows(child, windows);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_windows(item, windows);
            }
        }
        _ => {}
    }
}

fn parse_window(map: &serde_json::Map<String, Value>) -> Option<ParsedWindow> {
    let used_percent = numeric_field(
        map,
        &["used_percent", "usedPercent", "percent_used", "usage_percent", "usagePercent"],
    )
    .or_else(|| {
        let used = numeric_field(map, &["used", "consumed", "current_usage", "currentUsage"])?;
        let limit = numeric_field(map, &["limit", "max", "quota", "cap"])?;
        if limit <= 0.0 {
            return None;
        }
        Some((used / limit) * 100.0)
    })
    .or_else(|| {
        let remaining = numeric_field(map, &["remaining"])?;
        let limit = numeric_field(map, &["limit", "max", "quota", "cap"])?;
        if limit <= 0.0 {
            return None;
        }
        Some((1.0 - (remaining / limit)) * 100.0)
    })?;

    Some(ParsedWindow {
        used_percent: used_percent.clamp(0.0, 100.0),
        window_minutes: integer_field(
            map,
            &[
                "window_minutes",
                "windowMinutes",
                "window_mins",
                "limit_window_minutes",
                "duration_minutes",
            ],
        )
        .or_else(|| {
            integer_field(
                map,
                &["window_seconds", "windowSeconds", "limit_window_seconds", "duration_seconds"],
            )
            .map(|seconds| (seconds + 59) / 60)
        }),
        resets_at: reset_field(
            map,
            &["resets_at", "resetsAt", "reset_at", "reset_time", "window_reset_at", "ends_at"],
        ),
    })
}

fn numeric_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| map.get(*key))
        .and_then(|value| match value {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.parse::<f64>().ok(),
            _ => None,
        })
}

fn integer_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| map.get(*key))
        .and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.parse::<i64>().ok(),
            _ => None,
        })
}

fn reset_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| map.get(*key)).and_then(|value| match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|date| date.timestamp()),
        _ => None,
    })
}

fn extract_plan_type(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in ["plan_type", "plan", "tier", "subscription_tier", "type", "loginMethod", "login_method"] {
                if let Some(Value::String(text)) = map.get(key) {
                    if !text.trim().is_empty() {
                        return Some(text.clone());
                    }
                }
            }
            map.values().find_map(extract_plan_type)
        }
        Value::Array(items) => items.iter().find_map(extract_plan_type),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::Engine;
    use serde_json::Value;

    use super::{parse_gemini_id_token_claims, parse_usage_value, read_gemini_credentials_from_path};

    #[tokio::test]
    async fn reads_gemini_credentials_from_explicit_path() {
        let dir = std::env::temp_dir().join(format!("switchfetcher-gemini-creds-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        let path = dir.join("oauth_creds.json");
        fs::write(
            &path,
            r#"{
                "access_token": "access",
                "refresh_token": "refresh",
                "id_token": "header.payload.sig",
                "expiry_date": 1763000000000
            }"#,
        )
        .expect("credentials file should be written");

        let parsed = read_gemini_credentials_from_path(&path.to_string_lossy())
            .await
            .expect("credentials should parse");

        assert_eq!(parsed.access_token, "access");
        assert_eq!(parsed.refresh_token, "refresh");
        assert_eq!(parsed.id_token, "header.payload.sig");
        assert_eq!(parsed.expiry_date, 1763000000000);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn parses_email_and_client_id_from_gemini_id_token() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            r#"{"email":"user@example.com","aud":"client-id.apps.googleusercontent.com"}"#,
        );
        let token = format!("header.{payload}.signature");

        let (email, client_id) = parse_gemini_id_token_claims(&token);

        assert_eq!(email.as_deref(), Some("user@example.com"));
        assert_eq!(client_id.as_deref(), Some("client-id.apps.googleusercontent.com"));
    }

    #[test]
    fn parses_codexbar_style_gemini_usage_shape() {
        let value: Value = serde_json::from_str(
            r#"{
                "usage": {
                    "loginMethod": "Pro",
                    "primary": {
                        "usedPercent": 28.5,
                        "windowMinutes": 1440,
                        "resetsAt": "2026-01-25T00:00:00Z"
                    },
                    "secondary": {
                        "usedPercent": 14.2,
                        "windowMinutes": 10080,
                        "resetsAt": "2026-01-31T00:00:00Z"
                    }
                }
            }"#,
        )
        .expect("json should parse");

        let parsed = parse_usage_value("acc-1", &value).expect("usage should parse");

        assert_eq!(parsed.plan_type.as_deref(), Some("Pro"));
        assert_eq!(parsed.primary_used_percent, Some(28.5));
        assert_eq!(parsed.primary_window_minutes, Some(1440));
        assert_eq!(parsed.secondary_used_percent, Some(14.2));
        assert_eq!(parsed.secondary_window_minutes, Some(10080));
    }
}
