use serde_json::{json, Value};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
const CLAUDE_LOGIN_GUIDANCE: &str =
    "Claude Code is signed out. Run claude auth login --claudeai in Terminal, then reopen Commandeer.";
#[cfg(target_os = "macos")]
const CLAUDE_KEYCHAIN_UNLOCK_GUIDANCE: &str =
    "Claude Code's macOS login Keychain is locked. In Terminal, run security unlock-keychain ~/Library/Keychains/login.keychain-db and enter your Mac login password. Then run claude auth login --claudeai and reopen Commandeer.";

fn credentials_path() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "could not resolve home directory".to_string())?;
    Ok(PathBuf::from(home)
        .join(".claude")
        .join(".credentials.json"))
}

/// Claude Code's OAuth credentials JSON (`{"claudeAiOauth":{...}}`), read from
/// the file at ~/.claude/.credentials.json (Linux/Windows).
fn credentials_from_file() -> Result<String, String> {
    let path = credentials_path()?;
    std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
}

/// macOS stores the same credentials JSON in the login Keychain (generic
/// password, service "Claude Code-credentials") rather than a file. Read it via
/// the `security` CLI on purpose: the Keychain ACL is evaluated against the
/// calling binary, and /usr/bin/security is a stable Apple binary, so a one-time
/// "Always Allow" persists. Reading in-process (keyring/security-framework)
/// would bind to commandeer's ad-hoc signature and re-prompt on every rebuild.
#[cfg(target_os = "macos")]
fn credentials_from_keychain() -> Result<String, String> {
    let output = std::process::Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
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

#[cfg(target_os = "macos")]
fn credentials_guidance(keychain_error: &str) -> &'static str {
    if keychain_error.contains("exit status: 51") {
        CLAUDE_KEYCHAIN_UNLOCK_GUIDANCE
    } else {
        CLAUDE_LOGIN_GUIDANCE
    }
}

/// The Claude Code OAuth credentials JSON. On macOS the token lives in the login
/// Keychain (with the file as a fallback for non-default setups); elsewhere it's
/// the ~/.claude/.credentials.json file.
fn read_credentials() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        credentials_from_keychain().or_else(|kc_err| {
            credentials_from_file().map_err(|file_err| {
                eprintln!("Claude credentials unavailable — keychain: {kc_err}; file: {file_err}");
                credentials_guidance(&kc_err).to_string()
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        credentials_from_file()
    }
}

fn normalized_window(payload: &Value, source_key: &str, kind: &str) -> Option<Value> {
    let window = payload.get(source_key)?.as_object()?;
    let percent = window.get("utilization")?.as_f64()?;
    let resets_at = window
        .get("resets_at")
        .and_then(Value::as_str)
        .unwrap_or_default();

    Some(json!({
        "kind": kind,
        "percent": percent,
        "severity": if percent >= 90.0 {
            "error"
        } else if percent >= 75.0 {
            "warning"
        } else {
            "normal"
        },
        "resets_at": resets_at,
        "scope": null,
    }))
}

/// Claude's OAuth endpoint historically returned a `limits` array, but now
/// exposes the same counters as top-level `five_hour` and `seven_day` windows.
/// Keep the frontend contract stable and accept either response shape.
fn normalize_usage_payload(payload: Value) -> Value {
    if payload.get("limits").is_some_and(Value::is_array) {
        return payload;
    }

    let limits = [
        normalized_window(&payload, "five_hour", "session"),
        normalized_window(&payload, "seven_day", "weekly_all"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    json!({ "limits": limits })
}

/// Fetch Claude plan usage (session + weekly limits) using the Claude Code
/// OAuth token. The token never leaves the Rust side; only the usage JSON
/// is returned to the webview.
#[tauri::command]
pub async fn claude_usage() -> Result<Value, String> {
    let raw = read_credentials()?;
    let creds: Value = serde_json::from_str(&raw).map_err(|e| format!("parse credentials: {e}"))?;
    let token = creds["claudeAiOauth"]["accessToken"]
        .as_str()
        .ok_or("no Claude OAuth access token found")?;

    let resp = reqwest::Client::new()
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(token)
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| format!("usage request failed: {e}"))?;

    let status = resp.status();
    if status == 429 {
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300);
        return Err(format!("rate limited; retry after {retry_after}s"));
    }
    if !status.is_success() {
        return Err(format!("usage endpoint returned {}", status));
    }
    let payload = resp
        .json::<Value>()
        .await
        .map_err(|e| format!("parse usage response: {e}"))?;
    Ok(normalize_usage_payload(payload))
}

#[cfg(test)]
mod tests {
    use super::normalize_usage_payload;
    use serde_json::json;

    #[test]
    fn normalizes_current_usage_windows() {
        let normalized = normalize_usage_payload(json!({
            "five_hour": {
                "utilization": 24.5,
                "resets_at": "2026-08-08T17:00:00Z"
            },
            "seven_day": {
                "utilization": 81.0,
                "resets_at": "2026-08-12T09:00:00Z"
            },
            "seven_day_sonnet": null,
            "extra_usage": { "is_enabled": false }
        }));

        let limits = normalized["limits"].as_array().unwrap();
        assert_eq!(limits.len(), 2);
        assert_eq!(limits[0]["kind"], "session");
        assert_eq!(limits[0]["percent"], 24.5);
        assert_eq!(limits[0]["severity"], "normal");
        assert_eq!(limits[1]["kind"], "weekly_all");
        assert_eq!(limits[1]["percent"], 81.0);
        assert_eq!(limits[1]["severity"], "warning");
    }

    #[test]
    fn preserves_legacy_limits_payload() {
        let payload = json!({
            "limits": [{
                "kind": "session",
                "percent": 42,
                "severity": "normal",
                "resets_at": "2026-08-08T17:00:00Z",
                "scope": null
            }]
        });

        assert_eq!(normalize_usage_payload(payload.clone()), payload);
    }

    #[test]
    fn skips_absent_or_null_windows() {
        let normalized = normalize_usage_payload(json!({
            "five_hour": null,
            "seven_day": { "utilization": 12.0, "resets_at": null }
        }));

        let limits = normalized["limits"].as_array().unwrap();
        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0]["kind"], "weekly_all");
        assert_eq!(limits[0]["resets_at"], "");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keychain_interaction_failure_requires_unlock_before_login() {
        let guidance = super::credentials_guidance("keychain lookup failed (exit status: 51):");
        assert!(guidance.contains("security unlock-keychain"));
        assert!(guidance.contains("claude auth login --claudeai"));
    }
}
