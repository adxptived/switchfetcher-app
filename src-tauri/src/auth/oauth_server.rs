//! Local OAuth server for handling Codex ChatGPT login flow

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use tiny_http::{Header, Request, Response, Server};
use tokio::sync::oneshot;

use crate::types::{OAuthLoginInfo, StoredAccount};

const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_PORT: u16 = 1455; // Same as official Codex
const CALLBACK_HOST: &str = "127.0.0.1";
const TOKEN_EXCHANGE_TIMEOUT_SECONDS: u64 = 20;
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Login Successful — Switchfetcher</title>
    <meta charset="utf-8">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #0f1117;
            display: flex; justify-content: center; align-items: center;
            min-height: 100vh; color: #fff;
        }
        .card {
            background: #1a1d27;
            border: 1px solid #2a2d3a;
            border-radius: 16px;
            padding: 40px 48px;
            text-align: center;
            max-width: 400px; width: 90%;
            box-shadow: 0 24px 64px rgba(0,0,0,0.5);
        }
        .brand {
            display: flex; align-items: center; justify-content: center; gap: 8px;
            margin-bottom: 32px;
            font-size: 12px; font-weight: 600; letter-spacing: 0.1em;
            text-transform: uppercase; color: #6b7280;
        }
        .brand-dot { width: 8px; height: 8px; background: #10b981; border-radius: 50%; }
        .check {
            width: 64px; height: 64px;
            background: rgba(16,185,129,0.1); border: 2px solid #10b981;
            border-radius: 50%;
            display: flex; align-items: center; justify-content: center;
            margin: 0 auto 24px;
            font-size: 26px; color: #10b981;
        }
        h1 { font-size: 22px; font-weight: 700; color: #f9fafb; margin-bottom: 12px; }
        .email {
            display: inline-block;
            font-size: 13px; color: #10b981; font-weight: 500;
            background: rgba(16,185,129,0.08);
            padding: 5px 14px; border-radius: 20px; margin-bottom: 20px;
        }
        .hint { font-size: 13px; color: #6b7280; line-height: 1.5; }
        .countdown { margin-top: 14px; font-size: 12px; color: #4b5563; }
        .countdown span { color: #10b981; font-weight: 600; }
    </style>
</head>
<body>
    <div class="card">
        <div class="brand"><div class="brand-dot"></div>Switchfetcher</div>
        <div class="check">&#10003;</div>
        <h1>Login Successful!</h1>
        <div class="email">__EMAIL__</div>
        <p class="hint">You can close this window and return to Switchfetcher.</p>
        <p class="countdown">Closing in <span id="c">3</span>&#8230;</p>
    </div>
    <script>
        var n=3,el=document.getElementById('c');
        var t=setInterval(function(){n--;if(n<=0){clearInterval(t);window.close();}else{el.textContent=n;}},1000);
    </script>
</body>
</html>"#;
const FAILURE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Login Failed - Switchfetcher</title>
    <meta charset="utf-8">
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #0f1117;
            display: flex; justify-content: center; align-items: center;
            min-height: 100vh; color: #fff;
        }
        .card {
            background: #1a1d27;
            border: 1px solid #3a2230;
            border-radius: 16px;
            padding: 36px 40px;
            max-width: 520px; width: 92%;
            box-shadow: 0 24px 64px rgba(0,0,0,0.5);
        }
        .brand {
            display: flex; align-items: center; gap: 8px;
            margin-bottom: 24px;
            font-size: 12px; font-weight: 600; letter-spacing: 0.1em;
            text-transform: uppercase; color: #6b7280;
        }
        .brand-dot { width: 8px; height: 8px; background: #ef4444; border-radius: 50%; }
        h1 { font-size: 22px; font-weight: 700; color: #f9fafb; margin-bottom: 12px; }
        p { font-size: 14px; color: #d1d5db; line-height: 1.55; margin-bottom: 14px; }
        ul { margin: 0 0 14px 20px; color: #d1d5db; }
        li { margin-bottom: 8px; line-height: 1.45; }
        .details {
            margin-top: 18px;
            padding: 12px 14px;
            border-radius: 12px;
            background: rgba(239,68,68,0.08);
            border: 1px solid rgba(239,68,68,0.18);
            font-size: 12px;
            color: #fca5a5;
            word-break: break-word;
            white-space: pre-wrap;
        }
    </style>
</head>
<body>
    <div class="card">
        <div class="brand"><div class="brand-dot"></div>Switchfetcher</div>
        <h1>Login Failed</h1>
        <p>OpenAI returned a temporary authentication error while completing browser login.</p>
        <ul>
            <li>Try the login again in a few seconds.</li>
            <li>Clear cookies for chatgpt.com and auth.openai.com if the error keeps repeating.</li>
            <li>Disable VPN/proxy, or retry in an incognito window or another browser.</li>
        </ul>
        <p>You can close this tab and return to Switchfetcher to retry.</p>
        <div class="details">__DETAILS__</div>
    </div>
</body>
</html>"#;

/// PKCE codes for OAuth
#[derive(Debug, Clone)]
pub struct PkceCodes {
    pub code_verifier: String,
    pub code_challenge: String,
}

/// Generate PKCE codes
pub fn generate_pkce() -> PkceCodes {
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);

    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);

    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

/// Generate a random state parameter
fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Build the OAuth authorization URL
fn build_authorize_url(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkceCodes,
    state: &str,
) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "openid profile email offline_access"),
        ("code_challenge", &pkce.code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", "codex_cli_rs"), // Required by OpenAI OAuth
    ];

    let query_string = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    format!("{issuer}/oauth/authorize?{query_string}")
}

fn build_redirect_uri(port: u16) -> String {
    format!("http://localhost:{port}/auth/callback")
}

/// Token response from the OAuth server
#[derive(Debug, Clone, serde::Deserialize)]
struct TokenResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

/// Exchange authorization code for tokens
async fn exchange_code_for_tokens(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkceCodes,
    code: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(TOKEN_EXCHANGE_TIMEOUT_SECONDS))
        .build()
        .context("Failed to create OAuth HTTP client")?;

    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding::encode(code),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(client_id),
        urlencoding::encode(&pkce.code_verifier)
    );

    let resp = client
        .post(format!("{issuer}/oauth/token"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .context("Failed to send token request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Token exchange failed: {status} - {body}");
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .context("Failed to parse token response")?;
    Ok(tokens)
}

/// Parse claims from JWT ID token
///
/// This intentionally decodes the provider-issued ID token payload without local
/// signature verification because the desktop client only needs display claims
/// after the OAuth server exchange succeeds.
fn parse_id_token_claims(id_token: &str) -> (Option<String>, Option<String>, Option<String>) {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return (None, None, None);
    }

    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => return (None, None, None),
    };

    let json: serde_json::Value = match serde_json::from_slice(&payload) {
        Ok(v) => v,
        Err(_) => return (None, None, None),
    };

    let email = json.get("email").and_then(|v| v.as_str()).map(String::from);

    let auth_claims = json.get("https://api.openai.com/auth");

    let plan_type = auth_claims
        .and_then(|auth| auth.get("chatgpt_plan_type"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let account_id = auth_claims
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|v| v.as_str())
        .map(String::from);

    (email, plan_type, account_id)
}

/// OAuth login flow result
pub struct OAuthLoginResult {
    pub account: StoredAccount,
}

/// Start the OAuth login flow
pub async fn start_oauth_login(
    account_name: String,
) -> Result<(
    OAuthLoginInfo,
    oneshot::Receiver<Result<OAuthLoginResult>>,
    Arc<AtomicBool>,
)> {
    let pkce = generate_pkce();
    let state = generate_state();

    println!("[OAuth] Starting login for account: {account_name}");

    // Try official default port first; fall back to a random free port if it is busy.
    let server = match Server::http(format!("{CALLBACK_HOST}:{DEFAULT_PORT}")) {
        Ok(server) => server,
        Err(default_err) => {
            println!(
                "[OAuth] Default callback port {DEFAULT_PORT} unavailable ({default_err}), using a random local port"
            );
            Server::http(format!("{CALLBACK_HOST}:0")).map_err(|fallback_err| {
                anyhow::anyhow!(
                    "Failed to start OAuth server: default port {DEFAULT_PORT} error: {default_err}; fallback error: {fallback_err}"
                )
            })?
        }
    };

    let actual_port = match server.server_addr().to_ip() {
        Some(addr) => addr.port(),
        None => anyhow::bail!("Failed to determine server port"),
    };

    let redirect_uri = build_redirect_uri(actual_port);
    let auth_url = build_authorize_url(DEFAULT_ISSUER, CLIENT_ID, &redirect_uri, &pkce, &state);

    println!("[OAuth] Server started on port {actual_port}");
    println!("[OAuth] Authorization URL prepared");

    let login_info = OAuthLoginInfo {
        auth_url: auth_url.clone(),
        callback_port: actual_port,
    };

    // Create a channel for the result
    let (tx, rx) = oneshot::channel();
    let cancelled = Arc::new(AtomicBool::new(false));

    // Spawn the server in a background thread
    let server = Arc::new(server);
    let pkce_clone = pkce.clone();
    let state_clone = state.clone();
    let cancelled_clone = cancelled.clone();

    thread::spawn(move || {
        let result = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime.block_on(run_oauth_server(
                server,
                pkce_clone,
                state_clone,
                redirect_uri,
                account_name,
                cancelled_clone,
            )),
            Err(err) => Err(anyhow::anyhow!(
                "Failed to initialize OAuth runtime: {err}"
            )),
        };
        let _ = tx.send(result);
    });

    Ok((login_info, rx, cancelled))
}

/// Run the OAuth callback server
async fn run_oauth_server(
    server: Arc<Server>,
    pkce: PkceCodes,
    expected_state: String,
    redirect_uri: String,
    account_name: String,
    cancelled: Arc<AtomicBool>,
) -> Result<OAuthLoginResult> {
    let timeout = Duration::from_secs(300); // 5 minute timeout
    let start = std::time::Instant::now();

    loop {
        if cancelled.load(Ordering::Relaxed) {
            anyhow::bail!("OAuth login cancelled");
        }

        if start.elapsed() > timeout {
            anyhow::bail!("OAuth login timed out");
        }

        // Use recv_timeout to allow checking the timeout
        let request = match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(req)) => req,
            Ok(None) => continue,
            Err(_) => continue,
        };

        let result = handle_oauth_request(
            request,
            &pkce,
            &expected_state,
            &redirect_uri,
            &account_name,
        )
        .await;

        match result {
            HandleResult::Continue => continue,
            HandleResult::Success(account) => {
                server.unblock();
                return Ok(OAuthLoginResult { account });
            }
            HandleResult::Error(e) => {
                server.unblock();
                return Err(e);
            }
        }
    }
}

enum HandleResult {
    Continue,
    Success(StoredAccount),
    Error(anyhow::Error),
}

fn format_oauth_provider_error(error: &str, error_desc: &str) -> String {
    let normalized_error = error.trim();
    let normalized_desc = error_desc.trim();
    if normalized_error.eq_ignore_ascii_case("unknown_error") {
        return format!(
            "OpenAI authentication failed temporarily (unknown_error). Please try again. If it keeps happening, clear cookies for chatgpt.com/auth.openai.com, disable VPN/proxy, or retry in an incognito window. Provider details: {normalized_desc}"
        );
    }

    format!("OAuth error: {normalized_error} - {normalized_desc}")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn oauth_failure_response(details: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let html = FAILURE_HTML.replace("__DETAILS__", &escape_html(details));
    if let Ok(content_type) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
    {
        Response::from_string(html).with_header(content_type)
    } else {
        Response::from_string(html)
    }
}

async fn handle_oauth_request(
    request: Request,
    pkce: &PkceCodes,
    expected_state: &str,
    redirect_uri: &str,
    account_name: &str,
) -> HandleResult {
    let url_str = request.url().to_string();
    let parsed = match url::Url::parse(&format!("http://localhost{url_str}")) {
        Ok(u) => u,
        Err(_) => {
            let _ = request.respond(Response::from_string("Bad Request").with_status_code(400));
            return HandleResult::Continue;
        }
    };

    let path = parsed.path();

    if path == "/auth/callback" {
        println!("[OAuth] Received callback request");
        let params: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();

        println!(
            "[OAuth] Callback params: {:?}",
            params.keys().collect::<Vec<_>>()
        );

        // Check for error response
        if let Some(error) = params.get("error") {
            let error_desc = params
                .get("error_description")
                .map(|s| s.as_str())
                .unwrap_or("Unknown error");
            println!("[OAuth] Error from provider: {error} - {error_desc}");
            let message = format_oauth_provider_error(error, error_desc);
            let _ = request.respond(oauth_failure_response(&message).with_status_code(400));
            return HandleResult::Error(anyhow::anyhow!(message));
        }

        // Verify state
        if params.get("state").map(String::as_str) != Some(expected_state) {
            println!("[OAuth] State mismatch!");
            let _ = request.respond(Response::from_string("State mismatch").with_status_code(400));
            return HandleResult::Error(anyhow::anyhow!("OAuth state mismatch"));
        }

        println!("[OAuth] State verified OK");

        // Get the authorization code
        let code = match params.get("code") {
            Some(c) if !c.is_empty() => c.clone(),
            _ => {
                println!("[OAuth] Missing authorization code");
                let _ = request.respond(
                    Response::from_string("Missing authorization code").with_status_code(400),
                );
                return HandleResult::Error(anyhow::anyhow!("Missing authorization code"));
            }
        };

        println!("[OAuth] Got authorization code, exchanging for tokens...");

        // Exchange code for tokens
        match exchange_code_for_tokens(DEFAULT_ISSUER, CLIENT_ID, redirect_uri, pkce, &code).await {
            Ok(tokens) => {
                println!("[OAuth] Token exchange successful!");
                // Parse claims from ID token
                let (email, plan_type, chatgpt_account_id) =
                    parse_id_token_claims(&tokens.id_token);

                // Create the account
                let account = StoredAccount::new_chatgpt(
                    account_name.to_string(),
                    email,
                    plan_type,
                    tokens.id_token,
                    tokens.access_token,
                    tokens.refresh_token,
                    chatgpt_account_id,
                );

                // Send success response
                let email_display = account.email.as_deref().unwrap_or("Account added");
                let success_html = SUCCESS_HTML.replace("__EMAIL__", email_display);

                let response = if let Ok(content_type) = Header::from_bytes(
                    &b"Content-Type"[..],
                    &b"text/html; charset=utf-8"[..],
                ) {
                    Response::from_string(success_html).with_header(content_type)
                } else {
                    Response::from_string(success_html)
                };
                let _ = request.respond(response);

                return HandleResult::Success(account);
            }
            Err(e) => {
                println!("[OAuth] Token exchange failed: {e}");
                let _ = request.respond(
                    Response::from_string(format!("Token exchange failed: {e}"))
                        .with_status_code(500),
                );
                return HandleResult::Error(e);
            }
        }
    }

    // Handle other paths
    let _ = request.respond(Response::from_string("Not Found").with_status_code(404));
    HandleResult::Continue
}

/// Wait for the OAuth login to complete
pub async fn wait_for_oauth_login(
    rx: oneshot::Receiver<Result<OAuthLoginResult>>,
) -> Result<StoredAccount> {
    let result = rx.await.context("OAuth login was cancelled")??;
    Ok(result.account)
}

#[cfg(test)]
mod tests {
    use super::{build_redirect_uri, escape_html, format_oauth_provider_error};

    #[test]
    fn redirect_uri_uses_localhost_host() {
        assert_eq!(
            build_redirect_uri(1455),
            "http://localhost:1455/auth/callback"
        );
    }

    #[test]
    fn formats_unknown_error_with_retry_guidance() {
        let message = format_oauth_provider_error(
            "unknown_error",
            "Request ID abc-123. Try again later.",
        );

        assert!(message.contains("failed temporarily"));
        assert!(message.contains("Please try again"));
        assert!(message.contains("Request ID abc-123"));
    }

    #[test]
    fn escapes_html_in_failure_details() {
        assert_eq!(
            escape_html(r#"<tag attr="value">test & more</tag>"#),
            "&lt;tag attr=&quot;value&quot;&gt;test &amp; more&lt;/tag&gt;"
        );
    }
}
