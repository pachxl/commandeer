use reqwest::header::HeaderValue;
use serde::{Deserialize, Serialize};
use serde::de::Deserializer;
use serde_json::Value;
use std::path::PathBuf;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn home_dir() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "could not resolve home directory".to_string())?;
    Ok(PathBuf::from(home))
}

/// Codex CLI/Desktop credentials on Windows and Linux. `auth.json` is the
/// current format; the credentials path remains as a fallback for older builds.
fn credentials_from_file() -> Result<String, String> {
    let home = home_dir()?;
    let paths = [
        home.join(".codex").join("auth.json"),
        home.join(".Codex").join(".credentials.json"),
    ];

    let mut errors = Vec::new();
    for path in paths {
        match std::fs::read_to_string(&path) {
            Ok(raw) => return Ok(raw),
            Err(err) => errors.push(format!("{}: {err}", path.display())),
        }
    }
    Err(format!(
        "Codex credentials not found — {}",
        errors.join("; ")
    ))
}

/// macOS stores Codex's login in the login Keychain. Use the stable Apple
/// `security` binary so the user's one-time "Always Allow" choice survives
/// Commandeer rebuilds with a new ad-hoc signature.
#[cfg(target_os = "macos")]
fn credentials_from_keychain() -> Result<String, String> {
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Codex-credentials", "-w"])
        .output()
        .map_err(|e| format!("run security: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "keychain lookup failed ({}): {}",
            output.status,
            stderr.trim()
        ));
    }
    let raw = String::from_utf8(output.stdout)
        .map_err(|e| format!("keychain output not UTF-8: {e}"))?
        .trim()
        .to_string();
    if raw.is_empty() {
        return Err("keychain returned empty credentials".to_string());
    }
    Ok(raw)
}

fn read_credentials() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        credentials_from_keychain().or_else(|keychain_err| {
            credentials_from_file().map_err(|file_err| {
                format!("Codex credentials not found — keychain: {keychain_err}; file: {file_err}")
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        credentials_from_file()
    }
}

fn credential_field<'a>(credentials: &'a Value, name: &str) -> Option<&'a str> {
    credentials
        .get("tokens")
        .and_then(|tokens| tokens.get(name))
        .and_then(Value::as_str)
        .or_else(|| credentials.get(name).and_then(Value::as_str))
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CodexRateLimitWindow {
    pub used_percent: u32,
    pub limit_window_seconds: Option<u64>,
    pub reset_at: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CodexRateLimit {
    pub allowed: bool,
    pub limit_reached: bool,
    pub primary_window: Option<CodexRateLimitWindow>,
    pub secondary_window: Option<CodexRateLimitWindow>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CodexAdditionalRateLimit {
    pub limit_name: String,
    pub metered_feature: String,
    pub rate_limit: CodexRateLimit,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CodexCredits {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}

/// Deliberately contains only display-safe usage fields. The upstream response
/// also includes account identifiers and email; serde ignores them so they
/// never cross the Rust/webview boundary.
#[derive(Debug, Deserialize, Serialize)]
pub struct CodexUsage {
    pub plan_type: Option<String>,
    pub rate_limit: Option<CodexRateLimit>,
    // The endpoint uses both [] and null when there are no extra metered
    // limits. Keep the frontend contract stable by always serializing a list.
    #[serde(default, deserialize_with = "null_to_default")]
    pub additional_rate_limits: Vec<CodexAdditionalRateLimit>,
    pub credits: Option<CodexCredits>,
}

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Fetch Codex plan usage using the OAuth login maintained by Codex
/// CLI/Desktop. The token stays on the Rust side and only sanitized usage data
/// is returned to the webview.
#[tauri::command]
pub async fn codex_usage() -> Result<CodexUsage, String> {
    let raw = read_credentials()?;
    let credentials: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse Codex credentials: {e}"))?;
    let token = credential_field(&credentials, "access_token")
        .ok_or("no Codex OAuth access token found")?;
    let account_id = credential_field(&credentials, "account_id");

    let client = reqwest::Client::new();
    let mut request = client.get(USAGE_URL).bearer_auth(token);
    if let Some(account_id) = account_id {
        let value = HeaderValue::from_str(account_id)
            .map_err(|e| format!("invalid Codex account id: {e}"))?;
        request = request.header("ChatGPT-Account-Id", value);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Codex usage request failed: {e}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("Codex login expired — run `codex login` and try again".to_string());
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(300);
        return Err(format!("rate limited; retry after {retry_after}s"));
    }
    if !status.is_success() {
        return Err(format!("Codex usage endpoint returned {status}"));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("read Codex usage response: {e}"))?;
    serde_json::from_str::<CodexUsage>(&body)
        .map_err(|e| format!("parse Codex usage response: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_current_nested_credentials() {
        let credentials: Value =
            serde_json::from_str(r#"{"tokens":{"access_token":"token","account_id":"account"}}"#)
                .unwrap();
        assert_eq!(
            credential_field(&credentials, "access_token"),
            Some("token")
        );
        assert_eq!(
            credential_field(&credentials, "account_id"),
            Some("account")
        );
    }

    #[test]
    fn reads_legacy_top_level_credentials() {
        let credentials: Value =
            serde_json::from_str(r#"{"access_token":"token","account_id":"account"}"#).unwrap();
        assert_eq!(
            credential_field(&credentials, "access_token"),
            Some("token")
        );
        assert_eq!(
            credential_field(&credentials, "account_id"),
            Some("account")
        );
    }

    #[test]
    fn usage_response_drops_identity_fields() {
        let usage: CodexUsage = serde_json::from_str(
            r#"{
                "email":"private@example.com",
                "user_id":"private-user",
                "plan_type":"plus",
                "rate_limit":null,
                "additional_rate_limits":[],
                "credits":null
            }"#,
        )
        .unwrap();
        let serialized = serde_json::to_string(&usage).unwrap();
        assert!(!serialized.contains("private@example.com"));
        assert!(!serialized.contains("private-user"));
    }

    #[test]
    fn usage_response_accepts_null_additional_limits() {
        let usage: CodexUsage = serde_json::from_str(
            r#"{
                "plan_type":"plus",
                "rate_limit":{
                    "allowed":true,
                    "limit_reached":false,
                    "primary_window":{
                        "used_percent":7,
                        "limit_window_seconds":18000,
                        "reset_after_seconds":900,
                        "reset_at":1783944000
                    },
                    "secondary_window":null
                },
                "additional_rate_limits":null,
                "credits":{
                    "has_credits":false,
                    "unlimited":false,
                    "overage_limit_reached":false,
                    "balance":"0",
                    "approx_local_messages":[],
                    "approx_cloud_messages":[]
                }
            }"#,
        )
        .expect("live endpoint shape should decode");

        assert!(usage.additional_rate_limits.is_empty());
        assert_eq!(
            usage
                .rate_limit
                .and_then(|limit| limit.primary_window)
                .map(|window| window.used_percent),
            Some(7)
        );
    }
}
