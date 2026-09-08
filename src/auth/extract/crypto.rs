//! Cookie-decryption cryptography for browser token extraction.
//!
//! Chromium-based browsers (and the Slack desktop app) store the shared `d`
//! cookie (the `xoxd-…` value) *encrypted* inside their `Cookies` SQLite
//! database. On macOS the scheme is:
//!
//! 1. A per-browser AES password lives in the login Keychain as a generic
//!    password under the service name `"<Browser> Safe Storage"`
//!    (e.g. `"Chrome Safe Storage"`).
//! 2. That password is stretched into a 16-byte AES key with
//!    `PBKDF2-HMAC-SHA1(password, salt = b"saltysalt", iterations = 1003, dklen = 16)`.
//! 3. Each encrypted value is prefixed with the ASCII bytes `b"v10"`, followed
//!    by an AES-128-CBC ciphertext using an IV of sixteen space characters
//!    (`b"                "`) and PKCS#7 padding.
//! 4. Newer Chromium builds additionally prepend a 32-byte SHA-256 domain hash
//!    to the plaintext, which must be stripped to recover the raw cookie value.
//!
//! Only [`safe_storage_key`] is macOS-specific (it shells out to the system
//! `security` tool to read the Keychain, avoiding an extra dependency). The pure
//! AES/PKCS#7 logic in [`decrypt_cookie_value`] is platform independent so it can
//! be unit-tested everywhere.

use crate::error::{Result, SlackError};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};

#[cfg(target_os = "macos")]
use pbkdf2::pbkdf2_hmac;
#[cfg(target_os = "macos")]
use sha1::Sha1;
#[cfg(target_os = "macos")]
use std::process::Command;

/// Well-known Keychain account names used under a `"<App> Safe Storage"`
/// service. A single service can hold several items (for example the Slack
/// desktop app keeps both `"Slack Key"` for the direct-download build and
/// `"Slack App Store Key"` for a Mac App Store install), and only one of them
/// matches the profile whose cookies we are reading. We therefore try every
/// candidate account and let the caller keep whichever key actually decrypts.
#[cfg(target_os = "macos")]
const KNOWN_SAFE_STORAGE_ACCOUNTS: &[&str] = &["Slack Key", "Slack App Store Key", "Slack"];

/// AES-128-CBC decryptor used for Chromium cookie values.
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// ASCII prefix identifying the v10 encryption scheme.
const V10_PREFIX: &[u8] = b"v10";
/// Length of the SHA-256 domain-hash prefix newer Chromium prepends to plaintext.
const DOMAIN_HASH_LEN: usize = 32;
/// Every recovered Slack `d` cookie value begins with this marker.
const XOXD_PREFIX: &str = "xoxd-";
/// AES block size / derived-key length in bytes.
const AES_KEY_LEN: usize = 16;

// ---------------------------------------------------------------------------
// safe_storage_key
// ---------------------------------------------------------------------------

/// Derive every candidate AES key that might decrypt a profile's cookie values.
///
/// On macOS a `"<App> Safe Storage"` service can contain more than one generic
/// password (see [`KNOWN_SAFE_STORAGE_ACCOUNTS`]). Because
/// `security find-generic-password -s <service> -w` without an explicit account
/// returns only the *first* match — which is frequently the wrong one (e.g. a
/// stale Mac App Store key) — this queries each known account by name as well
/// as the account-less lookup, then derives a 16-byte key from each distinct
/// password via `PBKDF2-HMAC-SHA1(password, b"saltysalt", 1003)`.
///
/// The caller is expected to try each returned key in turn and keep whichever
/// one actually decrypts to a valid `xoxd-` value.
///
/// # Errors
///
/// Returns [`SlackError`] if no password could be read for the service at all
/// (for example the item does not exist or access was denied), or on any
/// non-macOS platform, where Keychain access is not yet implemented.
#[cfg(target_os = "macos")]
pub fn safe_storage_keys(safe_storage_service: &str) -> Result<Vec<Vec<u8>>> {
    let mut passwords: Vec<Vec<u8>> = Vec::new();
    let mut last_error = String::new();

    // Try the account-less lookup first (covers non-Slack browsers with a
    // single item), then each well-known Slack account name.
    let mut attempts: Vec<Option<&str>> = vec![None];
    attempts.extend(KNOWN_SAFE_STORAGE_ACCOUNTS.iter().map(|a| Some(*a)));

    for account in attempts {
        match read_keychain_password(safe_storage_service, account) {
            Ok(pw) if !pw.is_empty() => {
                if !passwords.contains(&pw) {
                    passwords.push(pw);
                }
            }
            Ok(_) => {}
            Err(e) => last_error = e,
        }
    }

    if passwords.is_empty() {
        return Err(SlackError::Other(format!(
            "failed to read any Keychain password for service '{safe_storage_service}': {last_error}"
        )));
    }

    Ok(passwords.iter().map(|pw| derive_key(pw)).collect())
}

/// Read one Keychain generic password, optionally scoped to `account`.
///
/// Returns the raw password bytes with any trailing newline stripped. An
/// unsuccessful lookup (missing item, denied access) is reported as `Err` with
/// the trimmed stderr so the caller can decide whether other candidates exist.
#[cfg(target_os = "macos")]
fn read_keychain_password(
    service: &str,
    account: Option<&str>,
) -> std::result::Result<Vec<u8>, String> {
    let mut args = vec!["find-generic-password"];
    if let Some(acct) = account {
        args.push("-a");
        args.push(acct);
    }
    args.push("-s");
    args.push(service);
    args.push("-w");

    let output = Command::new("security")
        .args(&args)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let mut password = output.stdout;
    while matches!(password.last(), Some(b'\n' | b'\r')) {
        password.pop();
    }
    Ok(password)
}

/// Non-macOS fallback: Keychain-backed key derivation is not implemented yet.
///
/// # Errors
///
/// Always returns [`SlackError::Other`]; kept so the crate compiles on every
/// target.
#[cfg(not(target_os = "macos"))]
pub fn safe_storage_keys(_safe_storage_service: &str) -> Result<Vec<Vec<u8>>> {
    Err(SlackError::Other(
        "browser cookie key derivation is only supported on macOS for now".into(),
    ))
}

/// Stretch a raw Keychain password into the 16-byte AES cookie key.
#[cfg(target_os = "macos")]
fn derive_key(password: &[u8]) -> Vec<u8> {
    let mut key = vec![0u8; AES_KEY_LEN];
    pbkdf2_hmac::<Sha1>(password, b"saltysalt", 1003, &mut key);
    key
}

// ---------------------------------------------------------------------------
// decrypt_cookie_value
// ---------------------------------------------------------------------------

/// Decrypt a Chromium `encrypted_value` blob into a Slack `xoxd-…` cookie.
///
/// The `encrypted_value` must start with the `b"v10"` marker. The remaining
/// bytes are decrypted with AES-128-CBC (IV = sixteen spaces) using the 16-byte
/// `key` from [`safe_storage_key`], PKCS#7 padding is removed, and — if the
/// resulting text does not already begin with `xoxd-` — a leading 32-byte
/// SHA-256 domain-hash prefix is stripped before the value is decoded as UTF-8.
///
/// # Errors
///
/// Returns [`SlackError`] if the `v10` prefix is missing, the key is not 16
/// bytes, the ciphertext is malformed, or the decrypted value is not a valid
/// `xoxd-` token.
pub fn decrypt_cookie_value(encrypted_value: &[u8], key: &[u8]) -> Result<String> {
    let ciphertext = encrypted_value.strip_prefix(V10_PREFIX).ok_or_else(|| {
        SlackError::Other("cookie value is not v10-encrypted (missing 'v10' prefix)".into())
    })?;

    let plaintext = decrypt_cbc(ciphertext, key)?;
    finalize_cookie_plaintext(plaintext)
}

/// AES-128-CBC decrypt with a fixed all-spaces IV and PKCS#7 unpadding.
fn decrypt_cbc(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
    let key: &[u8; AES_KEY_LEN] = key.try_into().map_err(|_| {
        SlackError::Other(format!(
            "cookie decryption key must be {AES_KEY_LEN} bytes, got {}",
            key.len()
        ))
    })?;

    if ciphertext.is_empty() || ciphertext.len() % AES_KEY_LEN != 0 {
        return Err(SlackError::Other(
            "cookie ciphertext length is not a multiple of the AES block size".into(),
        ));
    }

    let iv = [b' '; AES_KEY_LEN];
    let mut buf = ciphertext.to_vec();
    let plaintext = Aes128CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| SlackError::Other(format!("AES-CBC cookie decryption failed: {e}")))?;

    Ok(plaintext.to_vec())
}

/// Interpret decrypted cookie bytes, stripping the optional domain-hash prefix.
fn finalize_cookie_plaintext(plaintext: Vec<u8>) -> Result<String> {
    // Fast path: plaintext is already the raw `xoxd-…` value.
    if let Ok(s) = std::str::from_utf8(&plaintext) {
        if s.starts_with(XOXD_PREFIX) {
            return Ok(s.to_string());
        }
    }

    // Newer Chromium prepends a 32-byte SHA-256 domain hash; strip and retry.
    if plaintext.len() > DOMAIN_HASH_LEN {
        if let Ok(s) = std::str::from_utf8(&plaintext[DOMAIN_HASH_LEN..]) {
            if s.starts_with(XOXD_PREFIX) {
                return Ok(s.to_string());
            }
        }
    }

    Err(SlackError::Other(
        "decrypted cookie value does not look like an 'xoxd-' token".into(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};

    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

    /// Encrypt `plaintext` exactly as Chromium does and prepend the v10 marker.
    fn encrypt_v10(plaintext: &[u8], key: &[u8; AES_KEY_LEN]) -> Vec<u8> {
        let iv = [b' '; AES_KEY_LEN];
        // Round buffer up to the next block boundary so PKCS#7 always fits.
        let padded_len = (plaintext.len() / AES_KEY_LEN + 1) * AES_KEY_LEN;
        let mut buf = vec![0u8; padded_len];
        buf[..plaintext.len()].copy_from_slice(plaintext);

        let ciphertext = Aes128CbcEnc::new(key.into(), &iv.into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
            .expect("encryption fits in buffer");

        let mut out = V10_PREFIX.to_vec();
        out.extend_from_slice(ciphertext);
        out
    }

    #[test]
    fn decrypts_plain_xoxd_value_with_pkcs7_padding() {
        let key = [0x11u8; AES_KEY_LEN];
        let token = b"xoxd-abc123-not-a-real-token";
        let encrypted = encrypt_v10(token, &key);

        let got = decrypt_cookie_value(&encrypted, &key).expect("decrypts");
        assert_eq!(got, "xoxd-abc123-not-a-real-token");
    }

    #[test]
    fn strips_32_byte_domain_hash_prefix() {
        let key = [0x22u8; AES_KEY_LEN];
        // 32-byte fake SHA-256 domain hash (non-ASCII to exercise the strip path).
        let mut plaintext = vec![0xABu8; DOMAIN_HASH_LEN];
        plaintext.extend_from_slice(b"xoxd-withprefix-token");
        let encrypted = encrypt_v10(&plaintext, &key);

        let got = decrypt_cookie_value(&encrypted, &key).expect("decrypts");
        assert_eq!(got, "xoxd-withprefix-token");
    }

    #[test]
    fn prefers_raw_value_when_it_already_starts_with_xoxd() {
        // A value whose length exceeds 32 bytes but which is already a clean
        // xoxd- token must NOT have its first 32 bytes stripped.
        let key = [0x77u8; AES_KEY_LEN];
        let token = b"xoxd-this-is-definitely-longer-than-thirty-two-bytes-total";
        assert!(token.len() > DOMAIN_HASH_LEN);
        let encrypted = encrypt_v10(token, &key);

        let got = decrypt_cookie_value(&encrypted, &key).expect("decrypts");
        assert_eq!(got, std::str::from_utf8(token).unwrap());
    }

    #[test]
    fn missing_v10_prefix_errors() {
        let key = [0x33u8; AES_KEY_LEN];
        let err = decrypt_cookie_value(b"nope-not-encrypted", &key).unwrap_err();
        assert!(err.to_string().contains("v10"));
    }

    #[test]
    fn wrong_key_length_errors() {
        let key = [0x44u8; AES_KEY_LEN];
        let encrypted = encrypt_v10(b"xoxd-value", &key);
        let short_key = [0u8; 8];
        assert!(decrypt_cookie_value(&encrypted, &short_key).is_err());
    }

    #[test]
    fn non_xoxd_short_plaintext_errors() {
        let key = [0x55u8; AES_KEY_LEN];
        let encrypted = encrypt_v10(b"hello", &key);
        assert!(decrypt_cookie_value(&encrypted, &key).is_err());
    }

    #[test]
    fn non_block_aligned_ciphertext_errors() {
        let key = [0x66u8; AES_KEY_LEN];
        // v10 + 5 bytes that are not a multiple of the AES block size.
        let mut blob = V10_PREFIX.to_vec();
        blob.extend_from_slice(b"12345");
        assert!(decrypt_cookie_value(&blob, &key).is_err());
    }
}
