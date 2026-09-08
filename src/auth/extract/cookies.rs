//! Read the shared Slack `xoxd-` session cookie from a Chromium "Cookies" store.
//!
//! Chromium-based browsers (and the Slack desktop app) persist cookies in a
//! SQLite database. The Slack session cookie is stored under `name = 'd'`
//! against a `*.slack.com` host and its value is encrypted at rest. This module
//! reads the encrypted blob out of the database and hands it to
//! [`crate::auth::extract::crypto::decrypt_cookie_value`] for OS-specific
//! decryption.
//!
//! The database is opened **read-only** so we never mutate the user's live
//! browser profile. Because a running browser can hold a write lock on the
//! file, a locked database is handled gracefully by copying it (plus any
//! WAL/SHM sidecar files) to a temporary directory and reading the copy.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::auth::extract::crypto;
use crate::error::{Result, SlackError};

/// SQL selecting the encrypted value of the shared Slack `d` cookie.
const SELECT_D_COOKIE: &str = "SELECT encrypted_value FROM cookies \
     WHERE host_key LIKE '%slack.com' AND name = 'd' LIMIT 1";

/// Read and decrypt the shared Slack `xoxd-` cookie from a Chromium Cookies DB.
///
/// Opens `cookies_db` read-only, selects the encrypted value of the `d` cookie
/// scoped to a `*.slack.com` host, and decrypts it by trying each candidate in
/// `keys` (the browser's `"Safe Storage"` keys derived in
/// [`crate::auth::extract::crypto`]). A service can hold multiple keys (e.g. the
/// Slack app's `"Slack Key"` vs `"Slack App Store Key"`), so the first key that
/// yields a valid `xoxd-` value wins.
///
/// Returns:
/// - `Ok(Some(value))` when a Slack `d` cookie is present and decrypts.
/// - `Ok(None)` when the database has no matching cookie row.
/// - `Err(..)` when the database cannot be read, or none of `keys` decrypt it.
///
/// If the database is locked (e.g. the browser is running), it is copied to a
/// temporary directory and read from there so the live profile is left
/// untouched.
pub fn read_slack_d_cookie(cookies_db: &Path, keys: &[Vec<u8>]) -> Result<Option<String>> {
    let Some(encrypted) = read_encrypted_d_value(cookies_db)? else {
        return Ok(None);
    };

    let mut last_err: Option<SlackError> = None;
    for key in keys {
        match crypto::decrypt_cookie_value(&encrypted, key) {
            Ok(value) => return Ok(Some(value)),
            Err(e) => last_err = Some(e),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        SlackError::Other("no Safe Storage keys were available to decrypt the cookie".into())
    }))
}

/// Fetch the raw encrypted `d` cookie value, transparently handling a locked DB.
///
/// Attempts a direct read-only open first. If SQLite reports the database is
/// busy/locked, the file is copied to a scratch directory and read from there.
fn read_encrypted_d_value(cookies_db: &Path) -> Result<Option<Vec<u8>>> {
    match query_encrypted_value(cookies_db) {
        Ok(value) => Ok(value),
        Err(err) if is_locked(&err) => {
            // The browser holds a lock; read a private copy instead. The guard
            // (and its temp directory) lives until the query below completes.
            let guard = copy_db_to_temp(cookies_db)?;
            query_encrypted_value(&guard.db).map_err(map_sqlite)
        }
        Err(err) => Err(map_sqlite(err)),
    }
}

/// Open `path` read-only and run the `d`-cookie query, returning the raw bytes.
fn query_encrypted_value(path: &Path) -> rusqlite::Result<Option<Vec<u8>>> {
    // SQLITE_OPEN_READ_ONLY guarantees we never write to the profile. A plain
    // path (rather than a `file:` URI) avoids percent-encoding pitfalls with
    // paths that contain spaces, e.g. "Application Support".
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let mut stmt = conn.prepare(SELECT_D_COOKIE)?;
    stmt.query_row([], |row| row.get::<_, Vec<u8>>(0))
        .optional()
}

/// True if the error indicates the SQLite database was busy or locked.
fn is_locked(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(e, _)
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

/// Map a `rusqlite` error into the crate's error type.
fn map_sqlite(err: rusqlite::Error) -> SlackError {
    SlackError::Other(format!("failed to read cookies database: {err}"))
}

/// A copy of the Cookies DB in a scratch directory that is deleted on drop.
struct TempDbGuard {
    /// The scratch directory (removed recursively when dropped).
    dir: PathBuf,
    /// Path to the copied database inside `dir`.
    db: PathBuf,
}

impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Copy the Cookies database (and any `-wal`/`-shm` sidecars) into a unique
/// temporary directory so it can be opened even while the browser holds a lock.
fn copy_db_to_temp(src: &Path) -> Result<TempDbGuard> {
    let file_name = src
        .file_name()
        .ok_or_else(|| SlackError::Other("invalid cookies database path".into()))?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "slack-cli-cookies-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir)?;

    let db = dir.join(file_name);
    fs::copy(src, &db)?;

    // WAL-mode databases keep uncommitted pages in sidecar files; copy them so
    // the snapshot we read is consistent with the live database.
    for suffix in ["-wal", "-shm"] {
        let mut sibling = src.as_os_str().to_owned();
        sibling.push(suffix);
        let sibling = PathBuf::from(sibling);
        if sibling.exists() {
            let mut dest_name = file_name.to_owned();
            dest_name.push(suffix);
            let _ = fs::copy(&sibling, dir.join(dest_name));
        }
    }

    Ok(TempDbGuard { dir, db })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    /// Create a Chromium-shaped `cookies` table at `path` and insert `rows`.
    ///
    /// Each row is `(host_key, name, encrypted_value)`.
    fn make_cookies_db(path: &Path, rows: &[(&str, &str, &[u8])]) {
        let conn = Connection::open(path).expect("create sqlite db");
        conn.execute_batch(
            "CREATE TABLE cookies (
                 host_key        TEXT NOT NULL,
                 name            TEXT NOT NULL,
                 encrypted_value BLOB NOT NULL
             );",
        )
        .expect("create cookies table");
        for (host, name, enc) in rows {
            conn.execute(
                "INSERT INTO cookies (host_key, name, encrypted_value) VALUES (?1, ?2, ?3)",
                rusqlite::params![host, name, enc],
            )
            .expect("insert cookie row");
        }
    }

    #[test]
    fn query_returns_encrypted_bytes_for_matching_row() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("Cookies");
        let payload: &[u8] = b"v10encryptedblob";
        make_cookies_db(
            &db,
            &[
                ("app.slack.com", "d", payload),
                ("other.example.com", "d", b"nope"),
            ],
        );

        let got = query_encrypted_value(&db).expect("query ok");
        assert_eq!(got.as_deref(), Some(payload));
    }

    #[test]
    fn query_returns_none_when_no_slack_d_cookie() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("Cookies");
        // A 'd' cookie for a non-slack host and a non-'d' slack cookie: neither matches.
        make_cookies_db(
            &db,
            &[
                ("example.com", "d", b"x"),
                ("app.slack.com", "session", b"y"),
            ],
        );

        let got = query_encrypted_value(&db).expect("query ok");
        assert!(got.is_none());
    }

    #[test]
    fn read_slack_d_cookie_returns_none_when_absent() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("Cookies");
        make_cookies_db(&db, &[("app.slack.com", "session", b"y")]);

        // No matching row: crypto is never invoked and we get Ok(None).
        let got = read_slack_d_cookie(&db, &[vec![0u8; 16]]).expect("read ok");
        assert!(got.is_none());
    }

    #[test]
    fn read_slack_d_cookie_attempts_decrypt_for_matching_row() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("Cookies");
        // Bogus ciphertext: the query path succeeds and forwards the bytes to
        // the crypto layer, which is expected to reject the garbage value.
        make_cookies_db(&db, &[("app.slack.com", "d", b"not-really-encrypted")]);

        let result = read_slack_d_cookie(&db, &[vec![0u8; 16]]);
        assert!(
            result.is_err(),
            "decryption of a bogus value should error, got: {result:?}"
        );
    }

    #[test]
    fn read_slack_d_cookie_tries_multiple_keys() {
        use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        type Enc = cbc::Encryptor<aes::Aes128>;

        // Encrypt a real xoxd- value under one specific key.
        let good_key = [0x42u8; 16];
        let token = b"xoxd-multi-key-test-value";
        let iv = [b' '; 16];
        let padded_len = (token.len() / 16 + 1) * 16;
        let mut buf = vec![0u8; padded_len];
        buf[..token.len()].copy_from_slice(token);
        let ct = Enc::new(&good_key.into(), &iv.into())
            .encrypt_padded_mut::<Pkcs7>(&mut buf, token.len())
            .unwrap();
        let mut enc = b"v10".to_vec();
        enc.extend_from_slice(ct);

        let dir = tempdir().unwrap();
        let db = dir.path().join("Cookies");
        make_cookies_db(&db, &[("app.slack.com", "d", &enc)]);

        // A wrong key first, then the correct one: the second should win.
        let keys = vec![vec![0u8; 16], good_key.to_vec()];
        let got = read_slack_d_cookie(&db, &keys).expect("read ok");
        assert_eq!(got.as_deref(), Some("xoxd-multi-key-test-value"));
    }
}
