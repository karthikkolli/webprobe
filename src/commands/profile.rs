use anyhow::Result;
use clap::Subcommand;
use std::str::FromStr;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse, ProfileConfig};
use crate::types::{OutputFormat, ViewportSize};
use crate::webdriver::BrowserType;

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// Create a new profile
    Create {
        /// Profile name
        name: String,

        /// Browser type (chrome or firefox)
        #[arg(short, long, default_value = "chrome")]
        browser: String,

        /// Viewport size (e.g., "1920x1080")
        #[arg(short = 'v', long)]
        viewport: Option<String>,

        /// Run in visible mode (not headless)
        #[arg(short = 'n', long)]
        no_headless: bool,
    },

    /// Delete a profile
    Destroy {
        /// Profile name
        name: String,

        /// Force destroy even if locked
        #[arg(short, long)]
        force: bool,
    },

    /// List all profiles
    List {
        /// Output format
        #[arg(short, long, default_value = "simple")]
        format: OutputFormat,
    },

    /// Get information about a profile
    Info {
        /// Profile name
        name: String,

        /// Output format
        #[arg(short, long, default_value = "simple")]
        format: OutputFormat,
    },

    /// Lock a profile for exclusive access
    Lock {
        /// Profile name
        name: String,

        /// Lock duration in minutes
        #[arg(short, long, default_value = "30")]
        duration: u64,
    },

    /// Unlock a profile
    Unlock {
        /// Profile name
        name: String,
    },
}

pub async fn handle_profile(command: ProfileCommands) -> Result<()> {
    // Require daemon for all profile operations
    utils::require_daemon()?;

    match command {
        ProfileCommands::Create {
            name,
            browser,
            viewport,
            no_headless,
        } => {
            info!("Creating profile: {} for {}", name, browser);

            // Parse browser type
            let browser_type = BrowserType::from_str(&browser)?;

            // Parse viewport if provided
            let viewport_size = if let Some(v) = viewport {
                Some(ViewportSize::parse(&v)?)
            } else {
                None
            };

            let config = ProfileConfig {
                browser_type,
                viewport: viewport_size,
                headless: !no_headless,
                persist_cookies: true,
                persist_storage: true,
            };

            let request = DaemonRequest::CreateProfile {
                name: name.clone(),
                config,
            };

            match DaemonClient::send_request(request)? {
                DaemonResponse::Success(msg) => {
                    println!("✓ Profile '{}' created successfully", name);
                    if !msg.is_empty() && msg != format!("Profile '{}' created successfully", name)
                    {
                        println!("{}", msg);
                    }
                    Ok(())
                }
                DaemonResponse::Error(e) => {
                    eprintln!("✗ Failed to create profile '{}': {}", name, e);
                    Err(anyhow::anyhow!(e))
                }
                _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
            }
        }

        ProfileCommands::Destroy { name, force } => {
            info!("Destroying profile: {}", name);

            let request = DaemonRequest::DestroyProfile {
                name: name.clone(),
                force,
            };

            match DaemonClient::send_request(request)? {
                DaemonResponse::Success(msg) => {
                    println!("✓ Profile '{}' destroyed successfully", name);
                    if !msg.is_empty() && msg != format!("Profile '{}' destroyed", name) {
                        println!("{}", msg);
                    }
                    Ok(())
                }
                DaemonResponse::Error(e) => {
                    eprintln!("✗ Failed to destroy profile '{}': {}", name, e);
                    if !force && e.contains("locked") {
                        eprintln!("Hint: Use --force to destroy a locked profile");
                    }
                    Err(anyhow::anyhow!(e))
                }
                _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
            }
        }

        ProfileCommands::List { format } => {
            info!("Listing profiles");

            let request = DaemonRequest::ListProfiles;

            match DaemonClient::send_request(request)? {
                DaemonResponse::ProfileList(profiles) => {
                    if profiles.is_empty() {
                        println!("No profiles found");
                        return Ok(());
                    }

                    match format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&profiles)?);
                        }
                        OutputFormat::Simple => {
                            println!("Active Profiles:");
                            println!(
                                "{:<20} {:<10} {:<15} {:<10} {:<20}",
                                "Name", "Browser", "Viewport", "Tabs", "Last Accessed"
                            );
                            println!("{}", "-".repeat(75));

                            for profile in profiles {
                                let viewport_str = profile
                                    .viewport
                                    .map(|v| format!("{}x{}", v.width, v.height))
                                    .unwrap_or_else(|| "default".to_string());

                                let last_accessed = profile
                                    .last_accessed
                                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                                    .unwrap_or_else(|| "never".to_string());

                                let lock_indicator = if profile.locked_until.is_some() {
                                    "🔒"
                                } else {
                                    ""
                                };

                                println!(
                                    "{:<20} {:<10} {:<15} {:<10} {:<20}",
                                    format!("{}{}", lock_indicator, profile.name),
                                    format!("{:?}", profile.browser_type),
                                    viewport_str,
                                    profile.tabs_count,
                                    last_accessed
                                );
                            }
                        }
                    }
                    Ok(())
                }
                DaemonResponse::Error(e) => {
                    eprintln!("✗ Failed to list profiles: {}", e);
                    Err(anyhow::anyhow!(e))
                }
                _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
            }
        }

        ProfileCommands::Info { name, format } => {
            info!("Getting info for profile: {}", name);

            let request = DaemonRequest::ProfileInfo { name: name.clone() };

            match DaemonClient::send_request(request)? {
                DaemonResponse::ProfileMetadata(metadata) => {
                    match format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&metadata)?);
                        }
                        OutputFormat::Simple => {
                            println!("Profile: {}", metadata.name);
                            println!(
                                "Created: {}",
                                metadata.created_at.format("%Y-%m-%d %H:%M:%S")
                            );
                            println!("Created by: {}", metadata.created_by);
                            println!("Browser: {:?}", metadata.browser_type);

                            if let Some(viewport) = metadata.viewport {
                                println!("Viewport: {}x{}", viewport.width, viewport.height);
                            } else {
                                println!("Viewport: default");
                            }

                            if let Some(locked) = metadata.locked_until {
                                println!("Locked until: {}", locked.format("%Y-%m-%d %H:%M:%S"));
                            } else {
                                println!("Lock status: unlocked");
                            }

                            if let Some(accessed) = metadata.last_accessed {
                                println!("Last accessed: {}", accessed.format("%Y-%m-%d %H:%M:%S"));
                            } else {
                                println!("Last accessed: never");
                            }

                            println!("Active tabs: {}", metadata.tabs_count);
                        }
                    }
                    Ok(())
                }
                DaemonResponse::Error(e) => {
                    eprintln!("✗ Failed to get profile info for '{}': {}", name, e);
                    Err(anyhow::anyhow!(e))
                }
                _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
            }
        }

        ProfileCommands::Lock { name, duration } => {
            info!("Locking profile: {}", name);

            let request = DaemonRequest::LockProfile {
                name: name.clone(),
                duration_minutes: duration,
            };

            match DaemonClient::send_request(request)? {
                DaemonResponse::Success(msg) => {
                    println!("✓ {}", msg);
                    Ok(())
                }
                DaemonResponse::Error(e) => {
                    eprintln!("✗ Failed to lock profile '{}': {}", name, e);
                    Err(anyhow::anyhow!(e))
                }
                _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
            }
        }

        ProfileCommands::Unlock { name } => {
            info!("Unlocking profile: {}", name);

            let request = DaemonRequest::UnlockProfile { name: name.clone() };

            match DaemonClient::send_request(request)? {
                DaemonResponse::Success(msg) => {
                    println!("✓ {}", msg);
                    Ok(())
                }
                DaemonResponse::Error(e) => {
                    eprintln!("✗ Failed to unlock profile '{}': {}", name, e);
                    Err(anyhow::anyhow!(e))
                }
                _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
            }
        }
    }
}
