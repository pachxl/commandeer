//! Clipboard encryption at rest.
//!
//! On Windows we use DPAPI (CryptProtectData / CryptUnprotectData), which ties
//! the ciphertext to the current user account and requires no key management.
//! On Linux we use ChaCha20-Poly1305 with a random key held in the Secret
//! Service (login keyring) via the `keyring` crate, falling back to a
//! 0600-permission key file in the app data dir when no keyring daemon is
//! available. Rows are prefixed with a magic marker so pre-encryption rows
//! pass through `decrypt` unchanged (and get re-encrypted by a one-time
//! migration in db.rs). Other Unixes keep the plaintext passthrough.

#[cfg(target_os = "windows")]
pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN};

    let blob_in = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut blob_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &blob_in,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut blob_out,
        )
        .map_err(|e| format!("CryptProtectData failed: {e}"))?;

        let len = blob_out.cbData as usize;
        let out = std::slice::from_raw_parts(blob_out.pbData, len).to_vec();
        let _ = LocalFree(HLOCAL(blob_out.pbData as *mut _));
        Ok(out)
    }
}

#[cfg(target_os = "windows")]
pub fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN};

    let blob_in = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr() as *mut u8,
    };
    let mut blob_out = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &blob_in,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut blob_out,
        )
        .map_err(|e| format!("CryptUnprotectData failed: {e}"))?;

        let len = blob_out.cbData as usize;
        let out = std::slice::from_raw_parts(blob_out.pbData, len).to_vec();
        let _ = LocalFree(HLOCAL(blob_out.pbData as *mut _));
        Ok(out)
    }
}

/// Stored-blob marker for encrypted rows. Starts with a NUL byte, which never
/// begins a legacy plaintext row (they are non-empty UTF-8 clipboard strings).
#[cfg(target_os = "linux")]
const MAGIC: &[u8; 6] = b"\x00CMDR1";

/// Whether a stored blob is one of ours (vs legacy plaintext).
#[cfg(target_os = "linux")]
pub fn is_encrypted(blob: &[u8]) -> bool {
    blob.starts_with(MAGIC)
}

#[cfg(target_os = "linux")]
pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use chacha20poly1305::aead::{Aead, AeadCore, OsRng};
    use chacha20poly1305::{ChaCha20Poly1305, KeyInit};

    let cipher = ChaCha20Poly1305::new(key()?.as_slice().into());
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("clipboard encrypt failed: {e}"))?;

    let mut out = Vec::with_capacity(MAGIC.len() + nonce.len() + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

#[cfg(target_os = "linux")]
pub fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    use chacha20poly1305::aead::Aead;
    use chacha20poly1305::{ChaCha20Poly1305, KeyInit};

    if !is_encrypted(ciphertext) {
        // Row from before encryption landed on Linux; passed through so old
        // histories keep working (db.rs re-encrypts them on startup).
        return Ok(ciphertext.to_vec());
    }
    let body = &ciphertext[MAGIC.len()..];
    if body.len() < 12 {
        return Err("clipboard entry too short".to_string());
    }
    let (nonce, ct) = body.split_at(12);

    // Try the primary key, then any other key source present on disk / in the
    // keyring. A transient Secret Service outage can fall back to a file key
    // (or vice versa), leaving some rows encrypted under keyring and others
    // under the file; trying every available key keeps all rows decryptable.
    let mut candidates: Vec<[u8; 32]> = Vec::new();
    let mut push = |k: [u8; 32]| {
        if !candidates.contains(&k) {
            candidates.push(k);
        }
    };
    if let Ok(k) = key() {
        push(k);
    }
    if let Some(k) = existing_keyring_key() {
        push(k);
    }
    if let Some(k) = existing_file_key() {
        push(k);
    }
    if candidates.is_empty() {
        return Err("clipboard decrypt failed: no key available".to_string());
    }
    for k in &candidates {
        let cipher = ChaCha20Poly1305::new(k.as_slice().into());
        if let Ok(pt) = cipher.decrypt(nonce.into(), ct) {
            return Ok(pt);
        }
    }
    Err("clipboard decrypt failed: no matching key".to_string())
}

/// The 32-byte data key: from the Secret Service when available (created on
/// first use), else a 0600 key file next to the database. Resolved once.
#[cfg(target_os = "linux")]
fn key() -> Result<[u8; 32], String> {
    use std::sync::OnceLock;
    static KEY: OnceLock<Result<[u8; 32], String>> = OnceLock::new();
    KEY.get_or_init(|| keyring_key().or_else(|_| file_key())).clone()
}

#[cfg(target_os = "linux")]
fn generate_key() -> [u8; 32] {
    use chacha20poly1305::aead::rand_core::RngCore;
    let mut key = [0u8; 32];
    chacha20poly1305::aead::OsRng.fill_bytes(&mut key);
    key
}

#[cfg(target_os = "linux")]
fn keyring_key() -> Result<[u8; 32], String> {
    let entry =
        keyring::Entry::new("commandeer", "clipboard-key").map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(hex) => hex_decode(&hex).ok_or_else(|| "malformed key in keyring".to_string()),
        Err(keyring::Error::NoEntry) => {
            let key = generate_key();
            entry
                .set_password(&hex_encode(&key))
                .map_err(|e| e.to_string())?;
            Ok(key)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Read the keyring key if one already exists, without creating a new one.
/// Used at decrypt time to try an alternate key source.
#[cfg(target_os = "linux")]
fn existing_keyring_key() -> Option<[u8; 32]> {
    let entry = keyring::Entry::new("commandeer", "clipboard-key").ok()?;
    hex_decode(&entry.get_password().ok()?)
}

/// Read the key file if it already exists, without creating one.
#[cfg(target_os = "linux")]
fn existing_file_key() -> Option<[u8; 32]> {
    std::fs::read(key_file_path().ok()?)
        .ok()?
        .as_slice()
        .try_into()
        .ok()
}

/// Location of the fallback key file, alongside the clipboard db.
#[cfg(target_os = "linux")]
fn key_file_path() -> Result<std::path::PathBuf, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let data_home = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or(format!("{home}/.local/share"));
    Ok(std::path::PathBuf::from(data_home)
        .join("dev.commandeer.app")
        .join("clipboard.key"))
}

/// Fallback when no Secret Service daemon is running (headless, minimal WMs):
/// a raw key file readable only by the user, alongside the clipboard db.
#[cfg(target_os = "linux")]
fn file_key() -> Result<[u8; 32], String> {
    let path = key_file_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    if let Ok(bytes) = std::fs::read(&path) {
        return bytes
            .as_slice()
            .try_into()
            .map_err(|_| "malformed clipboard.key".to_string());
    }

    let key = generate_key();
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    f.write_all(&key).map_err(|e| e.to_string())?;
    eprintln!("clipboard: no Secret Service available, using key file at {}", path.display());
    Ok(key)
}

#[cfg(target_os = "linux")]
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(target_os = "linux")]
fn hex_decode(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(out)
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    Ok(plaintext.to_vec())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    Ok(ciphertext.to_vec())
}
