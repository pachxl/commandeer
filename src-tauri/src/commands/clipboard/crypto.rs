//! Clipboard encryption at rest.
//!
//! On Windows we use DPAPI (CryptProtectData / CryptUnprotectData), which ties
//! the ciphertext to the current user account and requires no key management.
//! On other platforms the data is stored as-is; this is a placeholder until a
//! suitable cross-platform keychain/keystore abstraction is added.

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

#[cfg(not(target_os = "windows"))]
pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    Ok(plaintext.to_vec())
}

#[cfg(not(target_os = "windows"))]
pub fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    Ok(ciphertext.to_vec())
}
