//! Daily-cached foreign-exchange rates for the calculator's currency
//! conversions. Rates come from the free Frankfurter API (ECB reference
//! rates, base USD), are cached under `<app-data>/rates.json`, and refresh at
//! most once per calendar day. When the network is unavailable the last good
//! cache is returned so offline conversions still work; if there is no cache
//! at all the error propagates and the frontend simply shows no result.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

use super::persistence::atomic_write;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rates {
    /// Base currency the rates are quoted against (always "USD" here).
    pub base: String,
    /// Reference date of the rates as reported upstream (YYYY-MM-DD).
    pub date: String,
    /// Currency code -> units of that currency per 1 unit of `base`. Includes
    /// the base itself at 1.0 so every code resolves with the same lookup.
    pub rates: HashMap<String, f64>,
    /// Days since the Unix epoch when we last fetched, used to throttle to one
    /// network refresh per day. Not part of the public API surface.
    #[serde(default)]
    fetched_day: u64,
}

/// Shape of the Frankfurter `/latest` response.
#[derive(Deserialize)]
struct FrankfurterResponse {
    base: String,
    date: String,
    rates: HashMap<String, f64>,
}

fn rates_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("rates.json"))
}

fn today_day() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

fn read_cache(path: &PathBuf) -> Option<Rates> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

async fn fetch_rates() -> Result<Rates, String> {
    let resp = reqwest::Client::new()
        .get("https://api.frankfurter.app/latest?from=USD")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("rates request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("rates endpoint returned {}", resp.status()));
    }
    let body: FrankfurterResponse = resp
        .json()
        .await
        .map_err(|e| format!("parse rates response: {e}"))?;
    let mut rates = body.rates;
    // Frankfurter omits the base from the rates map; add it so `USD` resolves.
    rates.entry(body.base.clone()).or_insert(1.0);
    Ok(Rates {
        base: body.base,
        date: body.date,
        rates,
        fetched_day: 0,
    })
}

/// Return today's FX rates, refreshing from the network at most once per day
/// and falling back to the cached copy when offline. Errors only when there is
/// neither a fresh fetch nor any cache to serve.
#[tauri::command]
pub async fn get_rates(app: tauri::AppHandle) -> Result<Rates, String> {
    let path = rates_path(&app)?;
    let today = today_day();

    if let Some(cache) = read_cache(&path) {
        if cache.fetched_day == today {
            return Ok(cache);
        }
    }

    match fetch_rates().await {
        Ok(mut fresh) => {
            fresh.fetched_day = today;
            if let Ok(json) = serde_json::to_string(&fresh) {
                let _ = atomic_write(&path, json);
            }
            Ok(fresh)
        }
        // Offline or upstream down: serve the last good cache if we have one.
        Err(e) => read_cache(&path).ok_or(e),
    }
}
