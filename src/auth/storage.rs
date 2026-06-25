//! Keyring storage for Slack CLI tokens
//!
//! Provides cross-platform token storage using the system keyring.
//! Tokens are stored as JSON-serialized TokenSet values.

use keyring::Entry;
use tracing::{debug, error, warn};

use super::TokenSet;
use crate::error::{Result, SlackError};

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

/// Key for storing the default workspace
const DEFAULT_KEY: &str = "default";

/// Prefix for workspace list entries
const WORKSPACE_LIST_KEY: &str = "workspaces";

/// Keyring-based token storage
///
/// Tokens are stored with keys like "token:<team_id>"
/// The default workspace is stored with key "default"
/// Workspace list is stored with key "workspaces"
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

    /// Store a token for a workspace
    ///
    /// Stores the token and verifies it can be read back immediately.
    /// Returns an error if storage or verification fails.
    pub fn store_token(team_id: &str, token: &TokenSet) -> Result<()> {
        let key = format!("token:{}", team_id);
        debug!(team_id = team_id, key = key, "Storing token in keyring");

        let entry = Self::entry(&key)?;
        let json = serde_json::to_string(token)?;
        debug!(team_id = team_id, json_len = json.len(), "Serialized token");

        entry.set_password(&json).map_err(|e| {
            error!(
                service = SERVICE_NAME,
                key = key,
                error = %e,
                "Failed to store token in keyring"
            );
            SlackError::Keyring(e)
        })?;
        debug!(team_id = team_id, "Token stored, verifying...");

        // Add to workspace list
        Self::add_to_workspace_list(team_id)?;

        // VERIFY: Read back the token to ensure it was stored
        match Self::get_token(team_id)? {
            Some(_) => {
                debug!(team_id = team_id, "Token storage verified successfully");
                Ok(())
            }
            None => {
                error!(
                    team_id = team_id,
                    "Token storage verification failed - token could not be retrieved after storing"
                );
                Err(SlackError::Other(
                    "Token storage verification failed - token could not be retrieved after storing. \
                     This may be a keyring access issue.".into()
                ))
            }
        }
    }

    /// Get token for a workspace
    pub fn get_token(team_id: &str) -> Result<Option<TokenSet>> {
        let key = format!("token:{}", team_id);
        debug!(team_id = team_id, key = key, "Getting token from keyring");
        let entry = Self::entry(&key)?;

        match entry.get_password() {
            Ok(json) => {
                debug!(
                    team_id = team_id,
                    json_len = json.len(),
                    "Retrieved token from keyring"
                );
                let token: TokenSet = serde_json::from_str(&json)?;
                Ok(Some(token))
            }
            Err(keyring::Error::NoEntry) => {
                debug!(team_id = team_id, "No token found in keyring");
                Ok(None)
            }
            Err(e) if backend_unavailable(&e) => {
                warn!(
                    service = SERVICE_NAME,
                    key = key,
                    error = %e,
                    "Keyring backend unavailable; treating as no stored token"
                );
                Ok(None)
            }
            Err(e) => {
                error!(
                    service = SERVICE_NAME,
                    key = key,
                    error = %e,
                    "Failed to get token from keyring"
                );
                Err(SlackError::Keyring(e))
            }
        }
    }

    /// Delete token for a workspace
    pub fn delete_token(team_id: &str) -> Result<()> {
        let key = format!("token:{}", team_id);
        debug!(team_id = team_id, key = key, "Deleting token from keyring");
        let entry = Self::entry(&key)?;

        match entry.delete_credential() {
            Ok(()) => {
                debug!(team_id = team_id, "Token deleted from keyring");
                // Remove from workspace list
                Self::remove_from_workspace_list(team_id)?;

                // If this was the default, clear the default
                if let Ok(Some(default)) = Self::get_default() {
                    if default == team_id {
                        let _ = Self::clear_default();
                    }
                }

                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                debug!(team_id = team_id, "Token already deleted (no entry)");
                Ok(())
            }
            Err(e) => {
                error!(
                    service = SERVICE_NAME,
                    key = key,
                    error = %e,
                    "Failed to delete token from keyring"
                );
                Err(SlackError::Keyring(e))
            }
        }
    }

    /// Set the default workspace
    ///
    /// Sets the default and verifies it can be read back.
    pub fn set_default(team_id: &str) -> Result<()> {
        debug!(team_id = team_id, "Setting default workspace in keyring");
        let entry = Self::entry(DEFAULT_KEY)?;
        entry.set_password(team_id).map_err(|e| {
            error!(
                service = SERVICE_NAME,
                key = DEFAULT_KEY,
                error = %e,
                "Failed to set default workspace in keyring"
            );
            SlackError::Keyring(e)
        })?;

        // VERIFY: Read back to ensure it was stored
        match Self::get_default()? {
            Some(stored) if stored == team_id => {
                debug!(team_id = team_id, "Default workspace set and verified");
                Ok(())
            }
            Some(stored) => {
                error!(
                    expected = team_id,
                    actual = stored,
                    "Default workspace verification failed - stored value doesn't match"
                );
                Err(SlackError::Other(
                    "Default workspace verification failed - stored value doesn't match".into(),
                ))
            }
            None => {
                error!(
                    team_id = team_id,
                    "Default workspace verification failed - could not be retrieved after storing"
                );
                Err(SlackError::Other(
                    "Default workspace verification failed - could not be retrieved after storing"
                        .into(),
                ))
            }
        }
    }

    /// Get the default workspace
    pub fn get_default() -> Result<Option<String>> {
        debug!("Getting default workspace from keyring");
        let entry = Self::entry(DEFAULT_KEY)?;

        match entry.get_password() {
            Ok(team_id) => {
                debug!(
                    team_id = team_id,
                    "Retrieved default workspace from keyring"
                );
                Ok(Some(team_id))
            }
            Err(keyring::Error::NoEntry) => {
                debug!("No default workspace set in keyring");
                Ok(None)
            }
            Err(e) if backend_unavailable(&e) => {
                warn!(
                    service = SERVICE_NAME,
                    key = DEFAULT_KEY,
                    error = %e,
                    "Keyring backend unavailable; treating as no default workspace"
                );
                Ok(None)
            }
            Err(e) => {
                error!(
                    service = SERVICE_NAME,
                    key = DEFAULT_KEY,
                    error = %e,
                    "Failed to get default workspace from keyring"
                );
                Err(SlackError::Keyring(e))
            }
        }
    }

    /// Clear the default workspace
    pub fn clear_default() -> Result<()> {
        debug!("Clearing default workspace from keyring");
        let entry = Self::entry(DEFAULT_KEY)?;

        match entry.delete_credential() {
            Ok(()) => {
                debug!("Default workspace cleared from keyring");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                debug!("Default workspace already cleared (no entry)");
                Ok(())
            }
            Err(e) => {
                error!(
                    service = SERVICE_NAME,
                    key = DEFAULT_KEY,
                    error = %e,
                    "Failed to clear default workspace from keyring"
                );
                Err(SlackError::Keyring(e))
            }
        }
    }

    /// List all stored workspaces
    ///
    /// Returns a list of team IDs that have stored tokens.
    /// This uses a separate keyring entry to track the list since
    /// keyring APIs don't support enumeration on all platforms.
    pub fn list_workspaces() -> Result<Vec<String>> {
        debug!("Listing workspaces from keyring");
        let entry = Self::entry(WORKSPACE_LIST_KEY)?;

        match entry.get_password() {
            Ok(json) => {
                let list: Vec<String> = serde_json::from_str(&json)?;
                debug!(count = list.len(), "Retrieved workspace list from keyring");
                Ok(list)
            }
            Err(keyring::Error::NoEntry) => {
                debug!("No workspace list found in keyring");
                Ok(vec![])
            }
            Err(e) if backend_unavailable(&e) => {
                warn!(
                    service = SERVICE_NAME,
                    key = WORKSPACE_LIST_KEY,
                    error = %e,
                    "Keyring backend unavailable; treating as empty workspace list"
                );
                Ok(vec![])
            }
            Err(e) => {
                error!(
                    service = SERVICE_NAME,
                    key = WORKSPACE_LIST_KEY,
                    error = %e,
                    "Failed to get workspace list from keyring"
                );
                Err(SlackError::Keyring(e))
            }
        }
    }

    /// Add a workspace to the list
    fn add_to_workspace_list(team_id: &str) -> Result<()> {
        debug!(team_id = team_id, "Adding workspace to list");
        let mut list = Self::list_workspaces()?;

        if !list.contains(&team_id.to_string()) {
            list.push(team_id.to_string());
            Self::save_workspace_list(&list)?;
        } else {
            debug!(team_id = team_id, "Workspace already in list");
        }

        Ok(())
    }

    /// Remove a workspace from the list
    fn remove_from_workspace_list(team_id: &str) -> Result<()> {
        debug!(team_id = team_id, "Removing workspace from list");
        let mut list = Self::list_workspaces()?;

        if let Some(pos) = list.iter().position(|x| x == team_id) {
            list.remove(pos);
            Self::save_workspace_list(&list)?;
            debug!(team_id = team_id, "Workspace removed from list");
        } else {
            debug!(team_id = team_id, "Workspace not in list");
        }

        Ok(())
    }

    /// Save the workspace list
    ///
    /// Saves the list and verifies it can be read back.
    fn save_workspace_list(list: &[String]) -> Result<()> {
        debug!(count = list.len(), "Saving workspace list to keyring");
        let entry = Self::entry(WORKSPACE_LIST_KEY)?;
        let json = serde_json::to_string(list)?;
        entry.set_password(&json).map_err(|e| {
            error!(
                service = SERVICE_NAME,
                key = WORKSPACE_LIST_KEY,
                error = %e,
                "Failed to save workspace list to keyring"
            );
            SlackError::Keyring(e)
        })?;

        // VERIFY: Read back to ensure it was stored
        let stored = Self::list_workspaces()?;
        if stored.len() == list.len() && stored.iter().all(|id| list.contains(id)) {
            debug!(count = list.len(), "Workspace list saved and verified");
            Ok(())
        } else {
            error!(
                expected = list.len(),
                actual = stored.len(),
                "Workspace list verification failed"
            );
            Err(SlackError::Other(
                "Workspace list verification failed - stored list doesn't match".into(),
            ))
        }
    }

    /// Get the token for the default workspace, or the first available workspace
    pub fn get_default_or_first() -> Result<Option<TokenSet>> {
        // Try default first
        if let Some(default_id) = Self::get_default()? {
            if let Some(token) = Self::get_token(&default_id)? {
                return Ok(Some(token));
            }
        }

        // Fall back to first workspace in list
        let workspaces = Self::list_workspaces()?;
        if let Some(first) = workspaces.first() {
            return Self::get_token(first);
        }

        Ok(None)
    }

    /// Get workspace info (team_id, team_name) for all stored workspaces
    pub fn get_workspace_info() -> Result<Vec<WorkspaceInfo>> {
        let team_ids = Self::list_workspaces()?;
        let default = Self::get_default()?;

        let mut info = Vec::new();
        for team_id in team_ids {
            if let Some(token) = Self::get_token(&team_id)? {
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

/// Information about a stored workspace
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceInfo {
    pub team_id: String,
    pub team_name: String,
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
    fn test_token_key_format() {
        let team_id = "T12345";
        let key = format!("token:{}", team_id);
        assert_eq!(key, "token:T12345");
    }

    #[test]
    fn test_workspace_info_serialization() {
        let info = WorkspaceInfo {
            team_id: "T12345".to_string(),
            team_name: "Test Workspace".to_string(),
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
