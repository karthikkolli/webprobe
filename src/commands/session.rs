use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Subcommand)]
pub enum SessionCommands {
    /// Create a new session
    Create {
        /// Session name
        name: String,

        /// Profile to use
        #[arg(short, long)]
        profile: Option<String>,

        /// Browser type
        #[arg(short, long, default_value = "chrome")]
        browser: String,
    },

    /// Destroy a session
    Destroy {
        /// Session name
        name: String,
    },

    /// List all sessions
    List,

    /// Show session details
    Show {
        /// Session name
        name: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionInfo {
    name: String,
    profile: Option<String>,
    browser: String,
    created_at: String,
    last_used: Option<String>,
}

fn sessions_dir() -> PathBuf {
    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("webprobe");
    path.push("sessions");
    path
}

fn session_file(name: &str) -> PathBuf {
    let mut path = sessions_dir();
    path.push(format!("{}.json", name));
    path
}

pub async fn handle_session(command: SessionCommands) -> Result<()> {
    // Ensure sessions directory exists
    fs::create_dir_all(sessions_dir())?;

    match command {
        SessionCommands::Create {
            name,
            profile,
            browser,
        } => {
            info!("Creating session: {}", name);

            let session_path = session_file(&name);
            if session_path.exists() {
                return Err(anyhow::anyhow!("Session '{}' already exists", name));
            }

            let session_info = SessionInfo {
                name: name.clone(),
                profile,
                browser,
                created_at: chrono::Utc::now().to_rfc3339(),
                last_used: None,
            };

            let json = serde_json::to_string_pretty(&session_info)?;
            fs::write(&session_path, json)?;

            println!("Session '{}' created", name);
            println!(
                "Use --session {} with any command to use this session",
                name
            );
        }
        SessionCommands::Destroy { name } => {
            info!("Destroying session: {}", name);

            let session_path = session_file(&name);
            if !session_path.exists() {
                return Err(anyhow::anyhow!("Session '{}' does not exist", name));
            }

            fs::remove_file(&session_path)?;

            // Also remove the profile directory if it exists
            let mut profile_path = sessions_dir();
            profile_path.push(&name);
            if profile_path.exists() {
                fs::remove_dir_all(&profile_path)?;
            }

            println!("Session '{}' destroyed", name);
        }
        SessionCommands::List => {
            let sessions_path = sessions_dir();
            if !sessions_path.exists() {
                println!("No sessions found");
                return Ok(());
            }

            let mut sessions = Vec::new();
            for entry in fs::read_dir(&sessions_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json")
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(info) = serde_json::from_str::<SessionInfo>(&content)
                {
                    sessions.push(info);
                }
            }

            if sessions.is_empty() {
                println!("No sessions found");
            } else {
                println!("Active sessions:");
                for session in sessions {
                    println!(
                        "  {} ({}) - created {}",
                        session.name, session.browser, session.created_at
                    );
                }
            }
        }
        SessionCommands::Show { name } => {
            let session_path = session_file(&name);
            if !session_path.exists() {
                return Err(anyhow::anyhow!("Session '{}' does not exist", name));
            }

            let content = fs::read_to_string(&session_path)?;
            let info: SessionInfo = serde_json::from_str(&content)?;

            println!("Session: {}", info.name);
            println!("Browser: {}", info.browser);
            if let Some(profile) = &info.profile {
                println!("Profile: {}", profile);
            }
            println!("Created: {}", info.created_at);
            if let Some(last_used) = &info.last_used {
                println!("Last used: {}", last_used);
            }
        }
    }
    Ok(())
}
