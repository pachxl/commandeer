use serde_json::Value;
use std::path::PathBuf;

fn credentials_path() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "could not resolve home directory".to_string())?;
    Ok(PathBuf::from(home).join(".claude").join(".credentials.json"))
}

/// Fetch Claude plan usage (session + weekly limits) using the Claude Code
/// OAuth token. The token never leaves the Rust side; only the usage JSON
/// is returned to the webview.
#[tauri::command]
pub async fn claude_usage() -> Result<Value, String> {
    let path = credentials_path()?;
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let creds: Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse credentials: {e}"))?;
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
