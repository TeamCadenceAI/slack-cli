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
//! [`KeyringStore::migrate_legacy_with`]).

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

/// Minimal credential backend used by the keyring storage logic.
///
/// Keeping the keyring crate behind this interface lets the state-management
/// and migration paths be exercised without touching a user's OS keyring.
trait SecretStore {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// Production [`SecretStore`] backed by the platform keyring.
struct SystemSecretStore;

impl SystemSecretStore {
    /// Create a new keyring entry.
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
}

impl SecretStore for SystemSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(SlackError::Keyring(e)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        Self::entry(key)?.set_password(value).map_err(|e| {
            error!(
                service = SERVICE_NAME,
                key = key,
                error = %e,
                "Failed to write keyring entry"
            );
            SlackError::Keyring(e)
        })
    }

    fn delete(&self, key: &str) -> Result<()> {
        Self::entry(key)?
            .delete_credential()
            .map_err(SlackError::Keyring)
    }
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

fn storage_unavailable(err: &SlackError) -> bool {
    matches!(err, SlackError::Keyring(source) if backend_unavailable(source))
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
    /// Load the consolidated blob, using the in-process cache when warm.
    fn load_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
    ) -> Result<KeyringData> {
        let mut guard = data_cache
            .lock()
            .map_err(|_| SlackError::Other("Failed to lock keyring cache".into()))?;
        if let Some(data) = guard.as_ref() {
            return Ok(data.clone());
        }

        let data = match store.get(STORE_KEY) {
            Ok(Some(json)) => serde_json::from_str(&json)?,
            Ok(None) => {
                // No new-format blob yet: migrate any legacy entries once.
                let migrated = Self::migrate_legacy_with(store).unwrap_or_default();
                if !migrated.workspaces.is_empty() {
                    // Persist the migrated blob FIRST so legacy credentials are
                    // never removed without a durable consolidated copy.
                    match Self::write_entry_with(store, &migrated) {
                        Ok(()) => Self::delete_legacy_entries_with(store, &migrated.workspaces),
                        Err(e) => {
                            warn!(error = %e, "Failed to persist migrated keyring blob; keeping legacy entries");
                        }
                    }
                }
                migrated
            }
            Err(e) if storage_unavailable(&e) => {
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
                return Err(e);
            }
        };

        *guard = Some(data.clone());
        Ok(data)
    }

    /// Write the blob to the keyring only (no cache interaction).
    fn write_entry_with<S: SecretStore>(store: &S, data: &KeyringData) -> Result<()> {
        let json = serde_json::to_string(data)?;
        store.set(STORE_KEY, &json)
    }

    /// Serialize and write the blob, updating the in-process cache.
    fn persist_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
        data: &KeyringData,
    ) -> Result<()> {
        Self::write_entry_with(store, data)?;
        if let Ok(mut guard) = data_cache.lock() {
            *guard = Some(data.clone());
        }
        Ok(())
    }

    /// Read, mutate, and persist the blob.
    fn update_with<S, F>(store: &S, data_cache: &Mutex<Option<KeyringData>>, f: F) -> Result<()>
    where
        S: SecretStore,
        F: FnOnce(&mut KeyringData),
    {
        let mut data = Self::load_with(store, data_cache)?;
        f(&mut data);
        Self::persist_with(store, data_cache, &data)
    }

    /// One-time migration from the legacy per-workspace layout.
    fn migrate_legacy_with<S: SecretStore>(store: &S) -> Result<KeyringData> {
        let ids: Vec<String> = match store.get(LEGACY_WORKSPACE_LIST_KEY) {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            Ok(None) => Vec::new(),
            Err(e) if storage_unavailable(&e) => return Ok(KeyringData::default()),
            Err(e) => return Err(e),
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
            match store.get(&key) {
                Ok(Some(json)) => {
                    if let Ok(token) = serde_json::from_str::<TokenSet>(&json) {
                        data.tokens.insert(team_id.clone(), token);
                        data.workspaces.push(team_id.clone());
                    }
                }
                Ok(None) => {}
                Err(e) if storage_unavailable(&e) => return Ok(KeyringData::default()),
                Err(e) => return Err(e),
            }
        }

        // Errors reading the optional legacy default never invalidate tokens.
        if let Ok(Some(default)) = store.get(LEGACY_DEFAULT_KEY) {
            if data.workspaces.contains(&default) {
                data.default = Some(default);
            }
        }

        Ok(data)
    }

    /// Best-effort deletion of legacy items after the blob has been written.
    fn delete_legacy_entries_with<S: SecretStore>(store: &S, team_ids: &[String]) {
        for team_id in team_ids {
            let _ = store.delete(&format!("token:{}", team_id));
        }
        let _ = store.delete(LEGACY_DEFAULT_KEY);
        let _ = store.delete(LEGACY_WORKSPACE_LIST_KEY);
    }

    /// Store a token for a workspace.
    pub fn store_token(team_id: &str, token: &TokenSet) -> Result<()> {
        Self::store_token_with(&SystemSecretStore, cache(), team_id, token)
    }

    fn store_token_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
        team_id: &str,
        token: &TokenSet,
    ) -> Result<()> {
        debug!(team_id = team_id, "Storing token in keyring blob");
        let token = token.clone();
        let team_id_owned = team_id.to_string();
        Self::update_with(store, data_cache, move |data| {
            data.tokens.insert(team_id_owned.clone(), token);
            if !data.workspaces.contains(&team_id_owned) {
                data.workspaces.push(team_id_owned);
            }
        })
    }

    /// Get token for a workspace.
    pub fn get_token(team_id: &str) -> Result<Option<TokenSet>> {
        Self::get_token_with(&SystemSecretStore, cache(), team_id)
    }

    fn get_token_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
        team_id: &str,
    ) -> Result<Option<TokenSet>> {
        debug!(team_id = team_id, "Getting token from keyring blob");
        Ok(Self::load_with(store, data_cache)?
            .tokens
            .get(team_id)
            .cloned())
    }

    /// Delete token for a workspace.
    pub fn delete_token(team_id: &str) -> Result<()> {
        Self::delete_token_with(&SystemSecretStore, cache(), team_id)
    }

    fn delete_token_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
        team_id: &str,
    ) -> Result<()> {
        debug!(team_id = team_id, "Deleting token from keyring blob");
        Self::update_with(store, data_cache, |data| {
            data.tokens.remove(team_id);
            data.workspaces.retain(|id| id != team_id);
            if data.default.as_deref() == Some(team_id) {
                data.default = None;
            }
        })
    }

    /// Set the default workspace.
    pub fn set_default(team_id: &str) -> Result<()> {
        Self::set_default_with(&SystemSecretStore, cache(), team_id)
    }

    fn set_default_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
        team_id: &str,
    ) -> Result<()> {
        debug!(
            team_id = team_id,
            "Setting default workspace in keyring blob"
        );
        let team_id_owned = team_id.to_string();
        Self::update_with(store, data_cache, move |data| {
            data.default = Some(team_id_owned);
        })
    }

    /// Get the default workspace.
    pub fn get_default() -> Result<Option<String>> {
        Self::get_default_with(&SystemSecretStore, cache())
    }

    fn get_default_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
    ) -> Result<Option<String>> {
        debug!("Getting default workspace from keyring blob");
        Ok(Self::load_with(store, data_cache)?.default)
    }

    /// Clear the default workspace.
    pub fn clear_default() -> Result<()> {
        Self::clear_default_with(&SystemSecretStore, cache())
    }

    fn clear_default_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
    ) -> Result<()> {
        debug!("Clearing default workspace from keyring blob");
        Self::update_with(store, data_cache, |data| data.default = None)
    }

    /// List all stored workspaces, in insertion order.
    pub fn list_workspaces() -> Result<Vec<String>> {
        Self::list_workspaces_with(&SystemSecretStore, cache())
    }

    fn list_workspaces_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
    ) -> Result<Vec<String>> {
        debug!("Listing workspaces from keyring blob");
        Ok(Self::load_with(store, data_cache)?.workspaces)
    }

    /// Get the token for the default workspace, or the first available workspace.
    pub fn get_default_or_first() -> Result<Option<TokenSet>> {
        Self::get_default_or_first_with(&SystemSecretStore, cache())
    }

    fn get_default_or_first_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
    ) -> Result<Option<TokenSet>> {
        let data = Self::load_with(store, data_cache)?;
        if let Some(default_id) = data.default.as_ref() {
            if let Some(token) = data.tokens.get(default_id) {
                return Ok(Some(token.clone()));
            }
        }
        if let Some(first) = data.workspaces.first() {
            return Ok(data.tokens.get(first).cloned());
        }
        Ok(None)
    }

    /// Get workspace information for all stored workspaces.
    pub fn get_workspace_info() -> Result<Vec<WorkspaceInfo>> {
        Self::get_workspace_info_with(&SystemSecretStore, cache())
    }

    fn get_workspace_info_with<S: SecretStore>(
        store: &S,
        data_cache: &Mutex<Option<KeyringData>>,
    ) -> Result<Vec<WorkspaceInfo>> {
        let data = Self::load_with(store, data_cache)?;
        Self::workspace_info_from_data(&data)
    }

    fn workspace_info_from_data(data: &KeyringData) -> Result<Vec<WorkspaceInfo>> {
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
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct MemorySecretStore {
        entries: Mutex<HashMap<String, String>>,
        operations: Mutex<Vec<String>>,
        failing_gets: Mutex<HashSet<String>>,
        fail_sets: Mutex<bool>,
    }

    impl MemorySecretStore {
        fn seed(&self, key: &str, value: impl Into<String>) {
            self.entries
                .lock()
                .unwrap()
                .insert(key.to_string(), value.into());
        }

        fn value(&self, key: &str) -> Option<String> {
            self.entries.lock().unwrap().get(key).cloned()
        }

        fn operations(&self) -> Vec<String> {
            self.operations.lock().unwrap().clone()
        }

        fn fail_get(&self, key: &str) {
            self.failing_gets.lock().unwrap().insert(key.to_string());
        }

        fn fail_sets(&self) {
            *self.fail_sets.lock().unwrap() = true;
        }
    }

    impl SecretStore for MemorySecretStore {
        fn get(&self, key: &str) -> Result<Option<String>> {
            self.operations.lock().unwrap().push(format!("get:{key}"));
            if self.failing_gets.lock().unwrap().contains(key) {
                return Err(SlackError::Other(format!("failed get: {key}")));
            }
            Ok(self.entries.lock().unwrap().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<()> {
            self.operations.lock().unwrap().push(format!("set:{key}"));
            if *self.fail_sets.lock().unwrap() {
                return Err(SlackError::Other("failed set".into()));
            }
            self.entries
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<()> {
            self.operations
                .lock()
                .unwrap()
                .push(format!("delete:{key}"));
            self.entries.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn test_cache() -> Mutex<Option<KeyringData>> {
        Mutex::new(None)
    }

    fn create_test_token(team_id: &str, team_name: &str) -> TokenSet {
        TokenSet {
            token_type: TokenType::UserOAuth,
            access_token: format!("xoxp-test-{team_id}"),
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
        assert_eq!(STORE_KEY, "store");
    }

    #[test]
    fn test_keyring_data_blob_roundtrips() {
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
        assert!(!json.contains("team_domain"));
    }

    #[test]
    fn test_entry_creation() {
        assert!(SystemSecretStore::entry("test_key").is_ok());
    }

    #[test]
    fn test_token_serialization_roundtrip() {
        let token = create_test_token("T12345", "Test Workspace");
        let json = serde_json::to_string(&token).unwrap();
        let deserialized: TokenSet = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.team_id, "T12345");
        assert_eq!(deserialized.team_name, "Test Workspace");
        assert_eq!(deserialized.access_token, "xoxp-test-T12345");
    }

    #[test]
    fn test_workspace_list_serialization() {
        let list = vec!["T1".to_string(), "T2".to_string(), "T3".to_string()];
        let json = serde_json::to_string(&list).unwrap();
        let deserialized: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, list);
    }

    #[test]
    fn test_store_and_get_token() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let token = create_test_token("T_TEST_001", "Test Workspace 1");

        KeyringStore::store_token_with(&store, &cache, "T_TEST_001", &token).unwrap();
        let retrieved = KeyringStore::get_token_with(&store, &cache, "T_TEST_001")
            .unwrap()
            .unwrap();

        assert_eq!(retrieved.team_id, "T_TEST_001");
        assert_eq!(retrieved.team_name, "Test Workspace 1");
        let persisted: KeyringData =
            serde_json::from_str(&store.value(STORE_KEY).unwrap()).unwrap();
        assert_eq!(persisted.workspaces, ["T_TEST_001"]);
    }

    #[test]
    fn test_get_nonexistent_token() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        assert!(KeyringStore::get_token_with(&store, &cache, "missing")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_delete_token() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let token = create_test_token("T_TEST_002", "Test Workspace 2");
        KeyringStore::store_token_with(&store, &cache, "T_TEST_002", &token).unwrap();
        KeyringStore::delete_token_with(&store, &cache, "T_TEST_002").unwrap();
        assert!(KeyringStore::get_token_with(&store, &cache, "T_TEST_002")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_default_workspace() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        KeyringStore::set_default_with(&store, &cache, "T1").unwrap();
        assert_eq!(
            KeyringStore::get_default_with(&store, &cache)
                .unwrap()
                .as_deref(),
            Some("T1")
        );
        KeyringStore::set_default_with(&store, &cache, "T2").unwrap();
        assert_eq!(
            KeyringStore::get_default_with(&store, &cache).unwrap(),
            Some("T2".into())
        );
        KeyringStore::clear_default_with(&store, &cache).unwrap();
        assert_eq!(
            KeyringStore::get_default_with(&store, &cache).unwrap(),
            None
        );
    }

    #[test]
    fn test_list_workspaces() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        for (id, name) in [("T1", "One"), ("T2", "Two")] {
            KeyringStore::store_token_with(&store, &cache, id, &create_test_token(id, name))
                .unwrap();
        }
        // Replacing a token must not duplicate or reorder the workspace.
        KeyringStore::store_token_with(&store, &cache, "T1", &create_test_token("T1", "Renamed"))
            .unwrap();
        assert_eq!(
            KeyringStore::list_workspaces_with(&store, &cache).unwrap(),
            ["T1", "T2"]
        );
    }

    #[test]
    fn test_get_default_or_first() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let first = create_test_token("T1", "One");
        let second = create_test_token("T2", "Two");
        KeyringStore::store_token_with(&store, &cache, "T1", &first).unwrap();
        KeyringStore::store_token_with(&store, &cache, "T2", &second).unwrap();

        assert_eq!(
            KeyringStore::get_default_or_first_with(&store, &cache)
                .unwrap()
                .unwrap()
                .team_id,
            "T1"
        );
        KeyringStore::set_default_with(&store, &cache, "T2").unwrap();
        assert_eq!(
            KeyringStore::get_default_or_first_with(&store, &cache)
                .unwrap()
                .unwrap()
                .team_id,
            "T2"
        );
        // A stale default falls back to the first workspace.
        KeyringStore::set_default_with(&store, &cache, "missing").unwrap();
        assert_eq!(
            KeyringStore::get_default_or_first_with(&store, &cache)
                .unwrap()
                .unwrap()
                .team_id,
            "T1"
        );
    }

    #[test]
    fn empty_store_is_cached_and_returns_no_default_token() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        assert!(KeyringStore::get_default_or_first_with(&store, &cache)
            .unwrap()
            .is_none());
        assert!(KeyringStore::list_workspaces_with(&store, &cache)
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .operations()
                .iter()
                .filter(|op| op.as_str() == "get:store")
                .count(),
            1
        );
    }

    #[test]
    fn delete_last_workspace_clears_default_and_persists_empty_blob() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let token = create_test_token("T1", "One");
        KeyringStore::store_token_with(&store, &cache, "T1", &token).unwrap();
        KeyringStore::set_default_with(&store, &cache, "T1").unwrap();
        KeyringStore::delete_token_with(&store, &cache, "T1").unwrap();

        let data: KeyringData = serde_json::from_str(&store.value(STORE_KEY).unwrap()).unwrap();
        assert!(data.tokens.is_empty());
        assert!(data.workspaces.is_empty());
        assert!(data.default.is_none());
    }

    #[test]
    fn workspace_info_uses_order_default_domain_and_skips_missing_tokens() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let mut token = create_test_token("T1", "One");
        token.team_domain = Some("one".into());
        let mut data = KeyringData::default();
        data.tokens.insert("T1".into(), token);
        data.workspaces = vec!["missing".into(), "T1".into()];
        data.default = Some("T1".into());
        store.seed(STORE_KEY, serde_json::to_string(&data).unwrap());

        let info = KeyringStore::get_workspace_info_with(&store, &cache).unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].team_id, "T1");
        assert_eq!(info[0].team_domain.as_deref(), Some("one"));
        assert!(info[0].is_default);
        assert_eq!(info[0].token_type, "UserOAuth");
    }

    #[test]
    fn legacy_layout_is_persisted_before_it_is_deleted() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let one = create_test_token("T1", "One");
        let two = create_test_token("T2", "Two");
        store.seed(
            LEGACY_WORKSPACE_LIST_KEY,
            serde_json::to_string(&vec!["T1", "T2"]).unwrap(),
        );
        store.seed("token:T1", serde_json::to_string(&one).unwrap());
        store.seed("token:T2", serde_json::to_string(&two).unwrap());
        store.seed(LEGACY_DEFAULT_KEY, "T2");

        let data = KeyringStore::load_with(&store, &cache).unwrap();
        assert_eq!(data.workspaces, ["T1", "T2"]);
        assert_eq!(data.default.as_deref(), Some("T2"));
        assert!(store.value(STORE_KEY).is_some());
        assert!(store.value("token:T1").is_none());
        assert!(store.value("token:T2").is_none());
        assert!(store.value(LEGACY_DEFAULT_KEY).is_none());
        assert!(store.value(LEGACY_WORKSPACE_LIST_KEY).is_none());

        let operations = store.operations();
        let persisted = operations.iter().position(|op| op == "set:store").unwrap();
        for key in ["token:T1", "token:T2", "default", "workspaces"] {
            let deleted = operations
                .iter()
                .position(|op| op == &format!("delete:{key}"))
                .unwrap();
            assert!(persisted < deleted);
        }
    }

    #[test]
    fn failed_migration_write_keeps_every_legacy_entry() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        store.seed(LEGACY_WORKSPACE_LIST_KEY, r#"["T1"]"#);
        store.seed(
            "token:T1",
            serde_json::to_string(&create_test_token("T1", "One")).unwrap(),
        );
        store.seed(LEGACY_DEFAULT_KEY, "T1");
        store.fail_sets();

        let data = KeyringStore::load_with(&store, &cache).unwrap();
        assert_eq!(data.workspaces, ["T1"]);
        assert!(store.value(STORE_KEY).is_none());
        assert!(store.value("token:T1").is_some());
        assert!(store.value(LEGACY_DEFAULT_KEY).is_some());
        assert!(store.value(LEGACY_WORKSPACE_LIST_KEY).is_some());
        assert!(!store
            .operations()
            .iter()
            .any(|op| op.starts_with("delete:")));
    }

    #[test]
    fn corrupt_blob_json_is_reported_without_populating_cache() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        store.seed(STORE_KEY, "{not-json");
        assert!(KeyringStore::load_with(&store, &cache).is_err());
        assert!(cache.lock().unwrap().is_none());
    }

    #[test]
    fn malformed_legacy_values_are_ignored() {
        let store = MemorySecretStore::default();
        store.seed(LEGACY_WORKSPACE_LIST_KEY, r#"["T1","T2"]"#);
        store.seed("token:T1", "not-json");
        store.seed(
            "token:T2",
            serde_json::to_string(&create_test_token("T2", "Two")).unwrap(),
        );
        store.seed(LEGACY_DEFAULT_KEY, "unknown");
        let data = KeyringStore::migrate_legacy_with(&store).unwrap();
        assert_eq!(data.workspaces, ["T2"]);
        assert!(data.default.is_none());

        let malformed_list = MemorySecretStore::default();
        malformed_list.seed(LEGACY_WORKSPACE_LIST_KEY, "not-json");
        assert!(KeyringStore::migrate_legacy_with(&malformed_list)
            .unwrap()
            .workspaces
            .is_empty());
    }

    #[test]
    fn backend_read_and_write_failures_are_handled() {
        let read_failure = MemorySecretStore::default();
        let read_cache = test_cache();
        read_failure.fail_get(STORE_KEY);
        assert!(KeyringStore::load_with(&read_failure, &read_cache).is_err());

        let migration_failure = MemorySecretStore::default();
        migration_failure.fail_get(LEGACY_WORKSPACE_LIST_KEY);
        assert!(KeyringStore::migrate_legacy_with(&migration_failure).is_err());

        let write_failure = MemorySecretStore::default();
        let write_cache = test_cache();
        write_failure.fail_sets();
        assert!(KeyringStore::store_token_with(
            &write_failure,
            &write_cache,
            "T1",
            &create_test_token("T1", "One")
        )
        .is_err());
    }

    #[test]
    fn poisoned_cache_is_reported() {
        let store = MemorySecretStore::default();
        let cache = test_cache();
        let _ = std::panic::catch_unwind(|| {
            let _guard = cache.lock().unwrap();
            panic!("poison cache");
        });
        assert!(KeyringStore::load_with(&store, &cache).is_err());
    }
}
