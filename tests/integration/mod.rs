//! Integration tests for Slack CLI
//!
//! These tests use assert_cmd to run the actual binary with mockito
//! to mock the Slack API responses.
//!
//! Test environment setup:
//! - SLACK_API_BASE_URL: Set to mockito server URL for API mocking
//! - SLACK_TOKEN_STORE_PATH: Set to temp file for token storage

mod common;

mod auth_storage_test;
mod auth_test;
mod channels_test;
mod files_test;
mod messages_test;
mod search_test;
