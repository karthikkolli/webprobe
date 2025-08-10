//! Browser profile management for session persistence

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

/// Metadata about a browser profile
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileMetadata {
    /// Profile name
    pub name: String,
    /// Browser type (firefox, chrome)
    pub browser: String,
    /// When the profile was created
    pub created_at: DateTime<Utc>,
    /// When the profile was last used
    pub last_used: DateTime<Utc>,
    /// Whether this is a temporary profile
    pub is_temporary: bool,
}

/// Manages browser profiles for session persistence
pub struct ProfileManager {
    profiles_dir: PathBuf,
}

impl ProfileManager {
    pub fn new() -> Result<Self> {
        let home_dir = dirs::home_dir().context("Unable to determine home directory")?;
        let profiles_dir = home_dir.join(".webprobe").join("profiles");

        // Create profiles directory if it doesn't exist
        fs::create_dir_all(&profiles_dir)?;

        Ok(ProfileManager { profiles_dir })
    }

    pub fn create_profile(&self, name: &str, browser: &str) -> Result<PathBuf> {
        let profile_path = self.profiles_dir.join(name);

        if profile_path.exists() {
            anyhow::bail!("Profile '{}' already exists", name);
        }

        fs::create_dir_all(&profile_path)?;

        let metadata = ProfileMetadata {
            name: name.to_string(),
            browser: browser.to_string(),
            created_at: Utc::now(),
            last_used: Utc::now(),
            is_temporary: false,
        };

        let metadata_path = profile_path.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(metadata_path, metadata_json)?;

        info!("Created profile '{}' for {}", name, browser);
        Ok(profile_path)
    }

    pub fn delete_profile(&self, name: &str) -> Result<()> {
        let profile_path = self.profiles_dir.join(name);

        if !profile_path.exists() {
            anyhow::bail!("Profile '{}' does not exist", name);
        }

        fs::remove_dir_all(&profile_path)?;
        info!("Deleted profile '{}'", name);
        Ok(())
    }

    pub fn list_profiles(&self) -> Result<Vec<ProfileMetadata>> {
        let mut profiles = Vec::new();

        for entry in fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let metadata_path = path.join("metadata.json");
                if metadata_path.exists() {
                    let metadata_json = fs::read_to_string(metadata_path)?;
                    let metadata: ProfileMetadata = serde_json::from_str(&metadata_json)?;
                    profiles.push(metadata);
                }
            }
        }

        profiles.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        Ok(profiles)
    }

    #[allow(dead_code)]
    pub fn get_profile_path(&self, name: &str) -> Result<PathBuf> {
        let profile_path = self.profiles_dir.join(name);

        if !profile_path.exists() {
            anyhow::bail!("Profile '{}' does not exist", name);
        }

        // Update last_used timestamp
        let metadata_path = profile_path.join("metadata.json");
        if metadata_path.exists() {
            let metadata_json = fs::read_to_string(&metadata_path)?;
            let mut metadata: ProfileMetadata = serde_json::from_str(&metadata_json)?;
            metadata.last_used = Utc::now();
            let updated_json = serde_json::to_string_pretty(&metadata)?;
            fs::write(metadata_path, updated_json)?;
        }

        Ok(profile_path)
    }

    pub fn cleanup_old_profiles(&self, days: u32) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);
        let mut deleted = 0;

        for entry in fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let metadata_path = path.join("metadata.json");
                if metadata_path.exists() {
                    let metadata_json = fs::read_to_string(&metadata_path)?;
                    let metadata: ProfileMetadata = serde_json::from_str(&metadata_json)?;

                    if metadata.is_temporary && metadata.last_used < cutoff {
                        fs::remove_dir_all(&path)?;
                        deleted += 1;
                        debug!("Cleaned up old profile: {}", metadata.name);
                    }
                }
            }
        }

        Ok(deleted)
    }

    pub fn create_temporary_profile(&self, browser: &str) -> Result<PathBuf> {
        let name = format!("tmp-{}", uuid::Uuid::new_v4());
        let profile_path = self.profiles_dir.join(&name);

        fs::create_dir_all(&profile_path)?;

        let metadata = ProfileMetadata {
            name: name.clone(),
            browser: browser.to_string(),
            created_at: Utc::now(),
            last_used: Utc::now(),
            is_temporary: true,
        };

        let metadata_path = profile_path.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(metadata_path, metadata_json)?;

        debug!("Created temporary profile: {}", name);
        Ok(profile_path)
    }
}

impl Drop for ProfileManager {
    fn drop(&mut self) {
        // Clean up old temporary profiles on drop
        if let Err(e) = self.cleanup_old_profiles(0) {
            debug!("Error cleaning up temporary profiles: {}", e);
        }
    }
}
