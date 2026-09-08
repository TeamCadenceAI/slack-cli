//! Keyring storage for Slack CLI tokens
//!
//! Provides cross-platform token storage using the system keyring.
//!
//! # Single-blob layout
//!
//! All state — every workspace's [`TokenSet`], the default workspace, and the
//! workspace ordering — lives in **one** keyring item (`slack-cli` /
//! [`STORE_KEY`]) as a single JSON [`KeyringData`] blob. macOS grants Keychain
//! access per item, so one item means the user is prompted at most once per
//! process (and "Always Allow" silences it thereafter). The blob is read once
//! and cached in-process for the lifetime of the command.
//!
//! Earlier versions stored one item per workspace (`token:<team_id>`) plus
//! separate `default` / `workspaces` items, which caused a Keychain prompt for
//! every workspace when listing or resolving `-w`. On first access the store
//! transparently migrates that legacy layout into the single blob (see
//! [`KeyringStore::migrate_legacy`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use keyring::Entry;
use tracing::{debug, error, warn};

use super::TokenSet;
use crate::error::{Result, SlackError};

/// Consolidated keyring state, serialized as one JSON blob under [`STORE_KEY`].
///
/// Field names match the file-backed store's on-disk shape so the two backends
/// stay interchangeable.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct KeyringData {
    /// All stored tokens, keyed by team ID.
    tokens: HashMap<String, TokenSet>,
    /// The default workspace team ID, if one is set.
    default: Option<String>,
    /// Workspace team IDs in insertion order (preserves list ordering).
    workspaces: Vec<String>,
}

/// Process-wide cache of the decoded blob so a single command reads the
/// keyring at most once. `None` (uninitialized) vs `Some(data)` (loaded).
fn cache() -> &'static Mutex<Option<KeyringData>> {
    static CACHE: OnceLock<Mutex<Option<KeyringData>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Returns `true` if a keyring error means the platform credential store is
/// unavailable or inaccessible (as opposed to simply having no entry).
///
/// On a machine where the OS secret store can't be reached — e.g. headless
/// Linux with no Secret Service (`org.freedesktop.secrets`) daemon, or a
/// locked/unavailable backend — *read* operations degrade to "no credentials
/// stored" so the CLI cleanly reports `auth_required` instead of surfacing a
/// raw platform error. *Write* operations still treat these as hard errors so
/// we never silently fail to persist a token.
fn backend_unavailable(err: &keyring::Error) -> bool {
    matches!(
        err,
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_)
    )
}

/// Service name for keyring entries
const SERVICE_NAME: &str = "slack-cli";

/// Key for the single consolidated data blob.
const STORE_KEY: &str = "store";

/// Legacy key for the default workspace (pre-single-blob layout).
const LEGACY_DEFAULT_KEY: &str = "default";

/// Legacy key for the workspace ID list (pre-single-blob layout).
const LEGACY_WORKSPACE_LIST_KEY: &str = "workspaces";

/// Keyring-based token storage.
///
/// All state lives in one keyring item (`STORE_KEY`) as a single JSON blob,
/// read once per process and cached. See the module docs for the rationale
/// and the legacy migration path.
pub struct KeyringStore;

impl KeyringStore {
    /// Create a new keyring entry
    fn entry(key: &str) -> Result<Entry> {
        debug!(service = SERVICE_NAME, key = key, "Creating keyring entry");
        Entry::new(SERVICE_NAME, key).map_err(|e| {
            error!(
                service = SERVICE_NAME,
                key = key,
                error = %e,
                "Failed to create keyring entry"
            );
            SlackError::Keyring(e)
        })
    }

    /// Load the consolidated blob, using the in-process cache when warm.
    ///
    /// On a cold cache this performs a single keyring read. If the new blob is
    /// absent it attempts a one-time migration from the legacy per-workspace
    /// layout; if that yields nothing (or the backend is unavailable) it
    /// returns an empty [`KeyringData`].
    fn load() -> Result<KeyringData> {
        let mut guard = cache()
            .lock()
            .map_err(|_| SlackError::Other("Failed to lock keyring cache".into()))?;
        if let Some(data) = guard.as_ref() {
            return Ok(data.clone());
        }

        let entry = Self::entry(STORE_KEY)?;
        let data = match entry.get_password() {
            Ok(json) => serde_json::from_str(&json)?,
            Err(keyring::Error::NoEntry) => {
                // No new-format blob yet: migrate any legacy entries once.
                let migrated = Self::migrate_legacy().unwrap_or_default();
                if !migrated.workspaces.is_empty() {
                    // Persist the migrated blob FIRST so we never delete the
                    // legacy entries without a durable copy. Only on a
                    // successful write do we clean up the old per-workspace
                    // items; if the write fails we leave the legacy layout
                    // intact and retry on the next run.
                    //
                    // NOTE: we hold the cache lock here, so we call the
                    // lock-free `write_entry` directly — `persist` would try to
                    // re-lock the (non-reentrant) cache mutex and deadlock.
                    match Self::write_entry(&migrated) {
                        Ok(()) => Self::delete_legacy_entries(&migrated.workspaces),
                        Err(e) => {
                            warn!(error = %e, "Failed to persist migrated keyring blob; keeping legacy entries");
                        }
                    }
                }
                migrated
            }
            Err(e) if backend_unavailable(&e) => {
                warn!(
                    service = SERVICE_NAME,
                    key = STORE_KEY,
                    error = %e,
                    "Keyring backend unavailable; treating as empty store"
                );
                KeyringData::default()
            }
            Err(e) => {
                error!(
                    service = SERVICE_NAME,
                    key = STORE_KEY,
                    error = %e,
                    "Failed to read keyring store"
                );
                return Err(SlackError::Keyring(e));
            }
        };

        *guard = Some(data.clone());
        Ok(data)
    }

    /// Write the blob to the keyring only (no cache interaction).
    ///
    /// A write failure is a hard error so a token is never silently dropped.
    /// Callers that do not already hold the cache lock should use
    /// [`Self::persist`] instead so the in-process cache stays consistent.
    fn write_entry(data: &KeyringData) -> Result<()> {
        let entry = Self::entry(STORE_KEY)?;
        let json = serde_json::to_string(data)?;
        entry.set_password(&json).map_err(|e| {
            error!(
                service = SERVICE_NAME,
                key = STORE_KEY,
                error = %e,
                "Failed to write keyring store"
            );
            SlackError::Keyring(e)
        })
    }

    /// Serialize and write the blob, updating the in-process cache.
    ///
    /// Must NOT be called while holding the cache lock (the mutex is
    /// non-reentrant); the cold-start migration path in [`Self::load`] writes
    /// via [`Self::write_entry`] for that reason.
    fn persist(data: &KeyringData) -> Result<()> {
        Self::write_entry(data)?;
        if let Ok(mut guard) = cache().lock() {
            *guard = Some(data.clone());
        }
        Ok(())
    }

    /// Read, mutate, and persist the blob atomically under the cache lock-free
    /// contract used elsewhere (load clones, we mutate the clone, then persist).
    fn update<F>(f: F) -> Result<()>
    where
        F: FnOnce(&mut KeyringData),
    {
        let mut data = Self::load()?;
        f(&mut data);
        Self::persist(&data)
    }

    /// One-time migration from the legacy per-workspace layout
    /// (`token:<team_id>` items plus `default` / `workspaces` items) into a
    /// single [`KeyringData`] blob.
    ///
    /// This is the *only* path that still reads the old per-workspace items,
    /// so it triggers the old multi-prompt behavior exactly once; afterwards
    /// the consolidated blob is used and the legacy items are best-effort
    /// deleted. Returns an empty value when there is nothing to migrate.
    fn migrate_legacy() -> Result<KeyringData> {
        // Legacy workspace list.
        let ids: Vec<String> = match Self::entry(LEGACY_WORKSPACE_LIST_KEY)?.get_password() {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(keyring::Error::NoEntry) => Vec::new(),
            Err(e) if backend_unavailable(&e) => return Ok(KeyringData::default()),
            Err(e) => return Err(SlackError::Keyring(e)),
        };
        if ids.is_empty() {
            return Ok(KeyringData::default());
        }

        debug!(
            count = ids.len(),
            "Migrating legacy keyring entries to blob"
        );
        let mut data = KeyringData::default();
        for team_id in &ids {
            let key = format!("token:{}", team_id);
            match Self::entry(&key)?.get_password() {
                Ok(json) => {
                    if let Ok(token) = serde_json::from_str::<TokenSet>(&json) {
                        data.tokens.insert(team_id.clone(), token);
                        data.workspaces.push(team_id.clone());
                    }
                }
                Err(keyring::Error::NoEntry) => {}
                Err(e) if backend_unavailable(&e) => return Ok(KeyringData::default()),
                Err(e) => return Err(SlackError::Keyring(e)),
            }
        }

        // Legacy default.
        if let Ok(default) = Self::entry(LEGACY_DEFAULT_KEY)?.get_password() {
            if data.workspaces.contains(&default) {
                data.default = Some(default);
            }
        }

        Ok(data)
    }

    /// Best-effort deletion of the legacy per-workspace items after the
    /// consolidated blob has been durably written. Failures are ignored: a
    /// stray legacy item is harmless (the blob is authoritative) and will not
    /// be re-migrated once the blob exists.
    fn delete_legacy_entries(team_ids: &[String]) {
        for team_id in team_ids {
            if let Ok(entry) = Self::entry(&format!("token:{}", team_id)) {
                let _ = entry.delete_credential();
            }
        }
        if let Ok(entry) = Self::entry(LEGACY_DEFAULT_KEY) {
            let _ = entry.delete_credential();
        }
        if let Ok(entry) = Self::entry(LEGACY_WORKSPACE_LIST_KEY) {
            let _ = entry.delete_credential();
        }
    }

    /// Store a token for a workspace
    ///
    /// Inserts the token into the consolidated blob (appending to the
    /// workspace ordering if new) and persists it.
    pub fn store_token(team_id: &str, token: &TokenSet) -> Result<()> {
        debug!(team_id = team_id, "Storing token in keyring blob");
        let token = token.clone();
        let team_id_owned = team_id.to_string();
        Self::update(move |data| {
            data.tokens.insert(team_id_owned.clone(), token);
            if !data.workspaces.contains(&team_id_owned) {
                data.workspaces.push(team_id_owned);
            }
        })
    }

    /// Get token for a workspace
    pub fn get_token(team_id: &str) -> Result<Option<TokenSet>> {
        debug!(team_id = team_id, "Getting token from keyring blob");
        Ok(Self::load()?.tokens.get(team_id).cloned())
    }

    /// Delete token for a workspace
    pub fn delete_token(team_id: &str) -> Result<()> {
        debug!(team_id = team_id, "Deleting token from keyring blob");
        Self::update(|data| {
            data.tokens.remove(team_id);
            data.workspaces.retain(|id| id != team_id);
            if data.default.as_deref() == Some(team_id) {
                data.default = None;
            }
        })
    }

    /// Set the default workspace
    pub fn set_default(team_id: &str) -> Result<()> {
        debug!(
            team_id = team_id,
            "Setting default workspace in keyring blob"
        );
        let team_id_owned = team_id.to_string();
        Self::update(move |data| {
            data.default = Some(team_id_owned);
        })
    }

    /// Get the default workspace
    pub fn get_default() -> Result<Option<String>> {
        debug!("Getting default workspace from keyring blob");
        Ok(Self::load()?.default)
    }

    /// Clear the default workspace
    pub fn clear_default() -> Result<()> {
        debug!("Clearing default workspace from keyring blob");
        Self::update(|data| {
            data.default = None;
        })
    }

    /// List all stored workspaces
    ///
    /// Returns the team IDs of all stored workspaces, in insertion order.
    pub fn list_workspaces() -> Result<Vec<String>> {
        debug!("Listing workspaces from keyring blob");
        Ok(Self::load()?.workspaces)
    }

    /// Get the token for the default workspace, or the first available workspace
    pub fn get_default_or_first() -> Result<Option<TokenSet>> {
        let data = Self::load()?;

        // Try the default first.
        if let Some(default_id) = data.default.as_ref() {
            if let Some(token) = data.tokens.get(default_id) {
                return Ok(Some(token.clone()));
            }
        }

        // Fall back to the first workspace in the list.
        if let Some(first) = data.workspaces.first() {
            return Ok(data.tokens.get(first).cloned());
        }

        Ok(None)
    }

    /// Get workspace info (team_id, team_name, domain, type) for all stored
    /// workspaces. Reads the blob once — no per-workspace keyring access.
    pub fn get_workspace_info() -> Result<Vec<WorkspaceInfo>> {
        let data = Self::load()?;
        let default = data.default.as_ref();

        let mut info = Vec::new();
        for team_id in &data.workspaces {
            if let Some(token) = data.tokens.get(team_id) {
                info.push(WorkspaceInfo {
                    team_id: token.team_id.clone(),
                    team_name: token.team_name.clone(),
                    team_domain: token.team_domain.clone(),
                    is_default: default == Some(team_id),
                    token_type: format!("{:?}", token.token_type),
                });
            }
        }

        Ok(info)
    }
}

/// Information about a stored workspace
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceInfo {
    pub team_id: String,
    pub team_name: String,
    /// Workspace domain (the `<sub>` in `<sub>.slack.com`), when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_domain: Option<String>,
    pub is_default: bool,
    pub token_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenType;

    // Helper to create a test token
    fn create_test_token(team_id: &str, team_name: &str) -> TokenSet {
        TokenSet {
            token_type: TokenType::UserOAuth,
            access_token: format!("xoxp-test-{}", team_id),
            xoxd_cookie: None,
            team_id: team_id.to_string(),
            team_name: team_name.to_string(),
            team_domain: None,
            user_id: "U12345".to_string(),
            created_at: chrono::Utc::now(),
            scopes: vec!["channels:read".to_string()],
        }
    }

    #[test]
    fn test_service_name() {
        assert_eq!(SERVICE_NAME, "slack-cli");
    }

    #[test]
    fn test_store_key_is_singular() {
        // The whole point of the single-blob layout: one keyring item.
        assert_eq!(STORE_KEY, "store");
    }

    #[test]
    fn test_keyring_data_blob_roundtrips() {
        // A KeyringData blob with multiple workspaces survives a JSON round
        // trip with tokens, default, and ordering intact — this is the single
        // value read from (and written to) the one keyring item.
        let mut data = KeyringData::default();
        data.tokens
            .insert("T1".into(), create_test_token("T1", "One"));
        data.tokens
            .insert("T2".into(), create_test_token("T2", "Two"));
        data.workspaces = vec!["T1".into(), "T2".into()];
        data.default = Some("T2".into());

        let json = serde_json::to_string(&data).unwrap();
        let back: KeyringData = serde_json::from_str(&json).unwrap();

        assert_eq!(back.workspaces, vec!["T1".to_string(), "T2".to_string()]);
        assert_eq!(back.default.as_deref(), Some("T2"));
        assert_eq!(back.tokens.len(), 2);
        assert_eq!(back.tokens["T1"].team_name, "One");
        assert_eq!(back.tokens["T2"].access_token, "xoxp-test-T2");
    }

    #[test]
    fn test_workspace_info_serialization() {
        let info = WorkspaceInfo {
            team_id: "T12345".to_string(),
            team_name: "Test Workspace".to_string(),
            team_domain: None,
            is_default: true,
            token_type: "UserOAuth".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("T12345"));
        assert!(json.contains("Test Workspace"));
        assert!(json.contains("true"));
    }

    // Test that entry creation works
    #[test]
    fn test_entry_creation() {
        let result = KeyringStore::entry("test_key");
        assert!(result.is_ok());
    }

    // Test token serialization/deserialization (the core logic)
    #[test]
    fn test_token_serialization_roundtrip() {
        let token = create_test_token("T12345", "Test Workspace");
        let json = serde_json::to_string(&token).unwrap();
        let deserialized: TokenSet = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.team_id, "T12345");
        assert_eq!(deserialized.team_name, "Test Workspace");
        assert_eq!(deserialized.access_token, "xoxp-test-T12345");
    }

    // Test workspace list serialization
    #[test]
    fn test_workspace_list_serialization() {
        let list = vec!["T1".to_string(), "T2".to_string(), "T3".to_string()];
        let json = serde_json::to_string(&list).unwrap();
        let deserialized: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, list);
    }

    // =========================================================================
    // Integration tests that use the REAL system keyring
    // =========================================================================
    //
    // These tests require a real platform keyring backend with cross-Entry
    // persistence. They are marked #[ignore] by default because:
    //
    // 1. They modify system state (store credentials in your keychain)
    // 2. They require platform-specific keyring access:
    //    - macOS: Keychain Access (may prompt for permission)
    //    - Windows: Credential Manager
    //    - Linux: Secret Service (e.g., gnome-keyring, KWallet)
    // 3. They will FAIL in sandboxed/CI environments without keyring access
    //
    // To run these tests:
    //   cargo test --lib -- --ignored
    //
    // These tests are NOT expected to pass in:
    // - Docker containers without keyring setup
    // - CI systems without credential storage
    // - Sandboxed environments (App Sandbox on macOS)
    //
    // The mock keyring backend (keyring::mock) does NOT support cross-Entry
    // persistence, so it cannot be used for these integration tests.
    // =========================================================================

    #[test]
    #[ignore]
    fn test_store_and_get_token() {
        let team_id = "T_TEST_001";
        let token = create_test_token(team_id, "Test Workspace 1");

        // Clean up any existing state first
        let _ = KeyringStore::delete_token(team_id);

        // Store
        KeyringStore::store_token(team_id, &token).expect("Failed to store token");

        // Get
        let retrieved = KeyringStore::get_token(team_id)
            .expect("Failed to get token")
            .expect("Token not found");

        assert_eq!(retrieved.team_id, team_id);
        assert_eq!(retrieved.team_name, "Test Workspace 1");

        // Cleanup
        KeyringStore::delete_token(team_id).expect("Failed to delete token");
    }

    #[test]
    #[ignore]
    fn test_get_nonexistent_token() {
        // Use a unique ID that definitely doesn't exist
        let result = KeyringStore::get_token("T_NONEXISTENT_999_UNIQUE");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    #[ignore]
    fn test_delete_token() {
        let team_id = "T_TEST_002";
        let token = create_test_token(team_id, "Test Workspace 2");

        // Clean up any existing state first
        let _ = KeyringStore::delete_token(team_id);

        // Store then delete
        KeyringStore::store_token(team_id, &token).expect("Failed to store token");
        KeyringStore::delete_token(team_id).expect("Failed to delete token");

        // Verify it's gone
        let retrieved = KeyringStore::get_token(team_id).expect("Failed to get token");
        assert!(retrieved.is_none());
    }

    #[test]
    #[ignore]
    fn test_default_workspace() {
        let team_id = "T_TEST_003";

        // Clean up any existing state first
        let _ = KeyringStore::clear_default();

        // Set default
        KeyringStore::set_default(team_id).expect("Failed to set default");

        // Get default
        let default = KeyringStore::get_default()
            .expect("Failed to get default")
            .expect("Default not found");
        assert_eq!(default, team_id);

        // Clear default
        KeyringStore::clear_default().expect("Failed to clear default");

        let default_after = KeyringStore::get_default().expect("Failed to get default");
        assert!(default_after.is_none());
    }

    #[test]
    #[ignore]
    fn test_list_workspaces() {
        let team_id_1 = "T_TEST_LIST_1";
        let team_id_2 = "T_TEST_LIST_2";

        // Clean up any existing state first
        let _ = KeyringStore::delete_token(team_id_1);
        let _ = KeyringStore::delete_token(team_id_2);

        let token1 = create_test_token(team_id_1, "Test 1");
        let token2 = create_test_token(team_id_2, "Test 2");

        // Store both
        KeyringStore::store_token(team_id_1, &token1).expect("Failed to store token 1");
        KeyringStore::store_token(team_id_2, &token2).expect("Failed to store token 2");

        // List
        let workspaces = KeyringStore::list_workspaces().expect("Failed to list workspaces");
        assert!(workspaces.contains(&team_id_1.to_string()));
        assert!(workspaces.contains(&team_id_2.to_string()));

        // Cleanup
        KeyringStore::delete_token(team_id_1).expect("Failed to delete token 1");
        KeyringStore::delete_token(team_id_2).expect("Failed to delete token 2");
    }

    #[test]
    #[ignore]
    fn test_get_default_or_first() {
        let team_id = "T_TEST_004";
        let token = create_test_token(team_id, "Test 4");

        // Clean up any existing state first
        let _ = KeyringStore::delete_token(team_id);
        let _ = KeyringStore::clear_default();

        // Store a token
        KeyringStore::store_token(team_id, &token).expect("Failed to store token");

        // Should find it as the first available
        let retrieved = KeyringStore::get_default_or_first()
            .expect("Failed to get default")
            .expect("No token found");
        assert_eq!(retrieved.team_id, team_id);

        // Cleanup
        KeyringStore::delete_token(team_id).expect("Failed to delete token");
    }
}
