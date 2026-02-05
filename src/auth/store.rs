//! Token storage abstraction for Slack CLI
//!
//! Provides a trait-based storage abstraction that allows switching between:
//! - System keyring (production, default)
//! - File-based storage (testing)
//!
//! The storage backend is selected via environment variable:
//! - SLACK_TOKEN_STORE_PATH: If set, uses file-based storage at the given path
//! - Otherwise: Uses system keyring

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use super::{KeyringStore, TokenSet, WorkspaceInfo};
use crate::error::{Result, SlackError};

/// Environment variable to override token storage location (for testing)
pub const TOKEN_STORE_PATH_ENV: &str = "SLACK_TOKEN_STORE_PATH";

/// Token storage trait for abstracting backend implementations
pub trait TokenStore: Send + Sync {
    /// Store a token for a workspace
    fn store_token(&self, team_id: &str, token: &TokenSet) -> Result<()>;

    /// Get token for a workspace
    fn get_token(&self, team_id: &str) -> Result<Option<TokenSet>>;

    /// Delete token for a workspace
    fn delete_token(&self, team_id: &str) -> Result<()>;

    /// Set the default workspace
    fn set_default(&self, team_id: &str) -> Result<()>;

    /// Get the default workspace
    fn get_default(&self) -> Result<Option<String>>;

    /// Clear the default workspace
    fn clear_default(&self) -> Result<()>;

    /// List all stored workspaces
    fn list_workspaces(&self) -> Result<Vec<String>>;

    /// Get the token for the default workspace, or the first available workspace
    fn get_default_or_first(&self) -> Result<Option<TokenSet>> {
        // Try default first
        if let Some(default_id) = self.get_default()? {
            if let Some(token) = self.get_token(&default_id)? {
                return Ok(Some(token));
            }
        }

        // Fall back to first workspace in list
        let workspaces = self.list_workspaces()?;
        if let Some(first) = workspaces.first() {
            return self.get_token(first);
        }

        Ok(None)
    }

    /// Get workspace info (team_id, team_name, token_type) for all stored workspaces
    fn get_workspace_info(&self) -> Result<Vec<WorkspaceInfo>> {
        let team_ids = self.list_workspaces()?;
        let default = self.get_default()?;

        let mut info = Vec::new();
        for team_id in team_ids {
            if let Some(token) = self.get_token(&team_id)? {
                info.push(WorkspaceInfo {
                    team_id: token.team_id,
                    team_name: token.team_name,
                    is_default: default.as_ref() == Some(&team_id),
                    token_type: format!("{:?}", token.token_type),
                });
            }
        }

        Ok(info)
    }
}

/// File-based token storage for testing
///
/// Stores tokens in a JSON file at the specified path.
/// Thread-safe via internal Mutex.
pub struct FileTokenStore {
    path: PathBuf,
    cache: Mutex<Option<FileStoreData>>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct FileStoreData {
    tokens: HashMap<String, TokenSet>,
    default: Option<String>,
    workspaces: Vec<String>,
}

impl FileTokenStore {
    /// Create a new file-based token store
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(None),
        }
    }

    /// Load data from file
    fn load(&self) -> Result<FileStoreData> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| SlackError::Other("Failed to acquire lock on token store".into()))?;

        if let Some(data) = cache.as_ref() {
            return Ok(data.clone());
        }

        let data = if self.path.exists() {
            let contents = fs::read_to_string(&self.path)?;
            serde_json::from_str(&contents)?
        } else {
            FileStoreData::default()
        };

        *cache = Some(data.clone());
        Ok(data)
    }

    /// Save data to file
    fn save(&self, data: &FileStoreData) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = serde_json::to_string_pretty(data)?;
        fs::write(&self.path, contents)?;

        // Update cache
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| SlackError::Other("Failed to acquire lock on token store".into()))?;
        *cache = Some(data.clone());

        Ok(())
    }
}

impl TokenStore for FileTokenStore {
    fn store_token(&self, team_id: &str, token: &TokenSet) -> Result<()> {
        let mut data = self.load()?;
        data.tokens.insert(team_id.to_string(), token.clone());

        if !data.workspaces.contains(&team_id.to_string()) {
            data.workspaces.push(team_id.to_string());
        }

        self.save(&data)
    }

    fn get_token(&self, team_id: &str) -> Result<Option<TokenSet>> {
        let data = self.load()?;
        Ok(data.tokens.get(team_id).cloned())
    }

    fn delete_token(&self, team_id: &str) -> Result<()> {
        let mut data = self.load()?;
        data.tokens.remove(team_id);
        data.workspaces.retain(|w| w != team_id);

        // Clear default if it was this workspace
        if data.default.as_ref() == Some(&team_id.to_string()) {
            data.default = None;
        }

        self.save(&data)
    }

    fn set_default(&self, team_id: &str) -> Result<()> {
        let mut data = self.load()?;
        data.default = Some(team_id.to_string());
        self.save(&data)
    }

    fn get_default(&self) -> Result<Option<String>> {
        let data = self.load()?;
        Ok(data.default)
    }

    fn clear_default(&self) -> Result<()> {
        let mut data = self.load()?;
        data.default = None;
        self.save(&data)
    }

    fn list_workspaces(&self) -> Result<Vec<String>> {
        let data = self.load()?;
        Ok(data.workspaces)
    }
}

/// Keyring-based token storage adapter
///
/// Wraps the existing KeyringStore to implement the TokenStore trait.
pub struct KeyringTokenStore;

impl KeyringTokenStore {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KeyringTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenStore for KeyringTokenStore {
    fn store_token(&self, team_id: &str, token: &TokenSet) -> Result<()> {
        KeyringStore::store_token(team_id, token)
    }

    fn get_token(&self, team_id: &str) -> Result<Option<TokenSet>> {
        KeyringStore::get_token(team_id)
    }

    fn delete_token(&self, team_id: &str) -> Result<()> {
        KeyringStore::delete_token(team_id)
    }

    fn set_default(&self, team_id: &str) -> Result<()> {
        KeyringStore::set_default(team_id)
    }

    fn get_default(&self) -> Result<Option<String>> {
        KeyringStore::get_default()
    }

    fn clear_default(&self) -> Result<()> {
        KeyringStore::clear_default()
    }

    fn list_workspaces(&self) -> Result<Vec<String>> {
        KeyringStore::list_workspaces()
    }
}

/// Get the appropriate token store based on environment configuration
///
/// Returns a file-based store if SLACK_TOKEN_STORE_PATH is set,
/// otherwise returns the system keyring store.
pub fn get_token_store() -> Box<dyn TokenStore> {
    if let Ok(path) = std::env::var(TOKEN_STORE_PATH_ENV) {
        Box::new(FileTokenStore::new(PathBuf::from(path)))
    } else {
        Box::new(KeyringTokenStore::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenType;
    use tempfile::TempDir;

    fn create_test_token(team_id: &str, team_name: &str) -> TokenSet {
        TokenSet {
            token_type: TokenType::UserOAuth,
            access_token: format!("xoxp-test-{}", team_id),
            xoxd_cookie: None,
            team_id: team_id.to_string(),
            team_name: team_name.to_string(),
            user_id: "U12345".to_string(),
            created_at: chrono::Utc::now(),
            scopes: vec!["channels:read".to_string()],
        }
    }

    #[test]
    fn test_file_store_store_and_get() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let team_id = "T12345";
        let token = create_test_token(team_id, "Test Workspace");

        // Store
        store.store_token(team_id, &token).unwrap();

        // Get
        let retrieved = store.get_token(team_id).unwrap().unwrap();
        assert_eq!(retrieved.team_id, team_id);
        assert_eq!(retrieved.team_name, "Test Workspace");
    }

    #[test]
    fn test_file_store_get_nonexistent() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let result = store.get_token("T_NONEXISTENT").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_file_store_delete() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let team_id = "T12345";
        let token = create_test_token(team_id, "Test Workspace");

        // Store then delete
        store.store_token(team_id, &token).unwrap();
        store.delete_token(team_id).unwrap();

        // Verify it's gone
        let result = store.get_token(team_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_file_store_default_workspace() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let team_id = "T12345";

        // Set default
        store.set_default(team_id).unwrap();

        // Get default
        let default = store.get_default().unwrap().unwrap();
        assert_eq!(default, team_id);

        // Clear default
        store.clear_default().unwrap();
        let default_after = store.get_default().unwrap();
        assert!(default_after.is_none());
    }

    #[test]
    fn test_file_store_list_workspaces() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let token1 = create_test_token("T1", "Workspace 1");
        let token2 = create_test_token("T2", "Workspace 2");

        store.store_token("T1", &token1).unwrap();
        store.store_token("T2", &token2).unwrap();

        let workspaces = store.list_workspaces().unwrap();
        assert!(workspaces.contains(&"T1".to_string()));
        assert!(workspaces.contains(&"T2".to_string()));
    }

    #[test]
    fn test_file_store_get_default_or_first() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let token1 = create_test_token("T1", "Workspace 1");
        let token2 = create_test_token("T2", "Workspace 2");

        store.store_token("T1", &token1).unwrap();
        store.store_token("T2", &token2).unwrap();
        store.set_default("T2").unwrap();

        // Should return default workspace
        let result = store.get_default_or_first().unwrap().unwrap();
        assert_eq!(result.team_id, "T2");
    }

    #[test]
    fn test_file_store_get_default_or_first_no_default() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let token1 = create_test_token("T1", "Workspace 1");
        store.store_token("T1", &token1).unwrap();

        // Should return first workspace
        let result = store.get_default_or_first().unwrap().unwrap();
        assert_eq!(result.team_id, "T1");
    }

    #[test]
    fn test_file_store_persistence() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");

        // Store with one instance
        {
            let store = FileTokenStore::new(store_path.clone());
            let token = create_test_token("T1", "Workspace 1");
            store.store_token("T1", &token).unwrap();
            store.set_default("T1").unwrap();
        }

        // Retrieve with another instance
        {
            let store = FileTokenStore::new(store_path);
            let result = store.get_token("T1").unwrap().unwrap();
            assert_eq!(result.team_id, "T1");

            let default = store.get_default().unwrap().unwrap();
            assert_eq!(default, "T1");
        }
    }

    #[test]
    fn test_delete_clears_default_if_matching() {
        let tmp_dir = TempDir::new().unwrap();
        let store_path = tmp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let token = create_test_token("T1", "Workspace 1");
        store.store_token("T1", &token).unwrap();
        store.set_default("T1").unwrap();

        // Delete the default workspace
        store.delete_token("T1").unwrap();

        // Default should be cleared
        let default = store.get_default().unwrap();
        assert!(default.is_none());
    }
}
