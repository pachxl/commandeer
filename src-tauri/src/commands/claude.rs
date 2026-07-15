use serde_json::Value;
use std::path::PathBuf;

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

/// The Claude Code OAuth credentials JSON. On macOS the token lives in the login
/// Keychain (with the file as a fallback for non-default setups); elsewhere it's
/// the ~/.claude/.credentials.json file.
fn read_credentials() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        credentials_from_keychain().or_else(|kc_err| {
            credentials_from_file().map_err(|file_err| {
                format!("Claude credentials not found — keychain: {kc_err}; file: {file_err}")
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        credentials_from_file()
    }
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
    resp.json::<Value>()
        .await
        .map_err(|e| format!("parse usage response: {e}"))
}
