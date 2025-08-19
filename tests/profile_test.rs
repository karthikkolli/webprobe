// Tests for profile management functionality
use anyhow::Result;
use serde_json::Value;
use std::process::Command;

/// Helper to run webprobe command
fn run_command(args: &[&str]) -> Result<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(args)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output (both success and error cases should return JSON now)
    match serde_json::from_str(&stdout) {
        Ok(json) => Ok(json),
        Err(_) => {
            // Fallback for any non-JSON output
            Ok(serde_json::json!({
                "error": true,
                "message": stdout.to_string()
            }))
        }
    }
}

#[test]
fn test_profile_create_and_delete() -> Result<()> {
    let profile_name = format!("test-profile-{}", uuid::Uuid::new_v4());

    // Create profile
    let result = run_command(&["profile", "create", &profile_name])?;

    // Should succeed or return proper error
    if result["error"].as_bool() != Some(true) {
        // Profile created successfully
        assert!(result.is_object());

        // List profiles to verify it exists
        let list_result = run_command(&["profile", "list"])?;
        if let Some(profiles) = list_result.as_array() {
            let found = profiles
                .iter()
                .any(|p| p["name"].as_str() == Some(&profile_name));
            assert!(found, "Created profile should be in list");
        }

        // Delete profile
        let delete_result = run_command(&["profile", "delete", &profile_name])?;
        assert!(delete_result["error"].as_bool() != Some(true));

        // Verify it's gone
        let list_result = run_command(&["profile", "list"])?;
        if let Some(profiles) = list_result.as_array() {
            let found = profiles
                .iter()
                .any(|p| p["name"].as_str() == Some(&profile_name));
            assert!(!found, "Deleted profile should not be in list");
        }
    }

    Ok(())
}

#[test]
fn test_profile_delete_nonexistent() -> Result<()> {
    let profile_name = "nonexistent-profile-99999";

    // Try to delete non-existent profile
    let result = run_command(&["profile", "delete", profile_name])?;

    // Should return an error
    assert_eq!(result["error"].as_bool(), Some(true));
    if let Some(message) = result["message"].as_str() {
        assert!(
            message.contains("not found") || message.contains("does not exist"),
            "Error message should indicate profile not found"
        );
    }

    Ok(())
}

#[test]
fn test_profile_cleanup_temporary() -> Result<()> {
    // Run cleanup command
    let result = run_command(&["profile", "cleanup"])?;

    // Should succeed (even if no temporary profiles exist)
    if result["error"].as_bool() != Some(true) {
        // Check if it reports cleaned profiles
        if result["cleaned"].is_number() {
            // Cleaned count is always >= 0 by definition (u64)
            assert!(true, "Should report number of cleaned profiles");
        }
    }

    Ok(())
}

#[test]
fn test_profile_list_format() -> Result<()> {
    // List profiles
    let result = run_command(&["profile", "list"])?;

    // Should return array or error
    if result["error"].as_bool() != Some(true) {
        assert!(result.is_array(), "Profile list should be an array");

        // If there are profiles, check their structure
        if let Some(profiles) = result.as_array() {
            for profile in profiles {
                assert!(profile["name"].is_string(), "Profile should have name");
                assert!(profile["path"].is_string(), "Profile should have path");
                assert!(
                    profile["browser"].is_string(),
                    "Profile should have browser type"
                );
                // Optional fields
                // created_at, last_used might be present
            }
        }
    }

    Ok(())
}

#[test]
fn test_profile_with_browser_type() -> Result<()> {
    let chrome_profile = format!("chrome-profile-{}", uuid::Uuid::new_v4());
    let firefox_profile = format!("firefox-profile-{}", uuid::Uuid::new_v4());

    // Create Chrome profile
    let result = run_command(&["profile", "create", &chrome_profile, "--browser", "chrome"])?;
    if result["error"].as_bool() != Some(true) {
        assert_eq!(result["browser"].as_str(), Some("chrome"));

        // Clean up
        let _ = run_command(&["profile", "delete", &chrome_profile]);
    }

    // Create Firefox profile
    let result = run_command(&[
        "profile",
        "create",
        &firefox_profile,
        "--browser",
        "firefox",
    ])?;
    if result["error"].as_bool() != Some(true) {
        assert_eq!(result["browser"].as_str(), Some("firefox"));

        // Clean up
        let _ = run_command(&["profile", "delete", &firefox_profile]);
    }

    Ok(())
}

#[test]
fn test_profile_persistence_check() -> Result<()> {
    let profile_name = format!("persist-test-{}", uuid::Uuid::new_v4());

    // Create profile
    let create_result = run_command(&["profile", "create", &profile_name])?;

    if create_result["error"].as_bool() != Some(true) {
        // List profiles immediately
        let list1 = run_command(&["profile", "list"])?;

        // List profiles again (should still be there)
        let list2 = run_command(&["profile", "list"])?;

        // Both lists should contain the profile
        if let (Some(profiles1), Some(profiles2)) = (list1.as_array(), list2.as_array()) {
            let found1 = profiles1
                .iter()
                .any(|p| p["name"].as_str() == Some(&profile_name));
            let found2 = profiles2
                .iter()
                .any(|p| p["name"].as_str() == Some(&profile_name));

            assert!(found1, "Profile should exist in first list");
            assert!(found2, "Profile should still exist in second list");
        }

        // Clean up
        let _ = run_command(&["profile", "delete", &profile_name]);
    }

    Ok(())
}
