//! Test keyring operations for debugging
//!
//! This utility tests basic keyring operations to help diagnose
//! authentication storage issues on the current system.
//!
//! Run with: cargo run --bin test_keyring

use keyring::Entry;

const SERVICE_NAME: &str = "slack-cli-test";

fn main() {
    println!("Keyring Test Utility for Slack CLI");
    println!("===================================");
    println!();
    println!("Service name: {}", SERVICE_NAME);
    println!();

    let accounts = vec![
        ("token:T12345", "test-token-value-12345"),
        ("default", "T12345"),
        ("workspaces", r#"["T12345","T67890"]"#),
    ];

    let mut all_passed = true;

    for (account, test_value) in &accounts {
        println!("Testing account: {}", account);
        println!("{}", "-".repeat(40));

        let entry = match Entry::new(SERVICE_NAME, account) {
            Ok(e) => {
                println!("  [OK] Created entry");
                e
            }
            Err(e) => {
                println!("  [FAIL] Failed to create entry: {:?}", e);
                all_passed = false;
                println!();
                continue;
            }
        };

        // Test write
        match entry.set_password(test_value) {
            Ok(_) => println!("  [OK] Write succeeded"),
            Err(e) => {
                println!("  [FAIL] Write failed: {:?}", e);
                all_passed = false;
                println!();
                continue;
            }
        }

        // Test read
        match entry.get_password() {
            Ok(val) => {
                if val == *test_value {
                    println!("  [OK] Read succeeded (value matches)");
                } else {
                    println!(
                        "  [FAIL] Read value mismatch: expected '{}', got '{}'",
                        test_value, val
                    );
                    all_passed = false;
                }
            }
            Err(e) => {
                println!("  [FAIL] Read failed: {:?}", e);
                all_passed = false;
            }
        }

        // Test delete (cleanup)
        match entry.delete_credential() {
            Ok(_) => println!("  [OK] Delete succeeded (cleanup)"),
            Err(e) => {
                println!("  [WARN] Delete failed: {:?}", e);
                // Not a critical failure, just cleanup
            }
        }

        println!();
    }

    // Test with the actual slack-cli service name
    println!("Testing actual service name: slack-cli");
    println!("{}", "-".repeat(40));

    let actual_entry = match Entry::new("slack-cli", "test-verification") {
        Ok(e) => {
            println!("  [OK] Created entry with actual service name");
            e
        }
        Err(e) => {
            println!("  [FAIL] Failed to create entry: {:?}", e);
            all_passed = false;
            println!();
            print_summary(all_passed);
            return;
        }
    };

    let test_val = "verification-test-value";
    match actual_entry.set_password(test_val) {
        Ok(_) => println!("  [OK] Write succeeded"),
        Err(e) => {
            println!("  [FAIL] Write failed: {:?}", e);
            all_passed = false;
        }
    }

    match actual_entry.get_password() {
        Ok(val) if val == test_val => println!("  [OK] Read succeeded (value matches)"),
        Ok(val) => {
            println!("  [FAIL] Read value mismatch: got '{}'", val);
            all_passed = false;
        }
        Err(e) => {
            println!("  [FAIL] Read failed: {:?}", e);
            all_passed = false;
        }
    }

    let _ = actual_entry.delete_credential();
    println!("  [OK] Cleanup complete");
    println!();

    print_summary(all_passed);
}

fn print_summary(all_passed: bool) {
    println!("===================================");
    if all_passed {
        println!("All tests PASSED!");
        println!();
        println!("Keyring is working correctly on this system.");
        println!("If `slack auth add` still fails, the issue may be elsewhere.");
    } else {
        println!("Some tests FAILED!");
        println!();
        println!("Keyring storage may not work on this system.");
        println!("Consider using file-based storage instead:");
        println!();
        println!("  export SLACK_TOKEN_STORE_PATH=~/.slack-tokens.json");
        println!("  slack auth add ...");
    }
    println!();

    // Print platform-specific hints
    #[cfg(target_os = "macos")]
    {
        println!("macOS troubleshooting:");
        println!("  - Check Keychain Access app for 'slack-cli' entries");
        println!("  - Verify the app is allowed to access the keychain");
        println!("  - Run: security find-generic-password -s slack-cli");
    }

    #[cfg(target_os = "linux")]
    {
        println!("Linux troubleshooting:");
        println!("  - Ensure a Secret Service daemon is running (gnome-keyring, KWallet)");
        println!("  - Check if running in a headless/SSH environment");
    }

    #[cfg(target_os = "windows")]
    {
        println!("Windows troubleshooting:");
        println!("  - Check Credential Manager for 'slack-cli' entries");
    }
}
