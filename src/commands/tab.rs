use anyhow::Result;
use clap::Subcommand;

use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

#[derive(Subcommand)]
pub enum TabCommands {
    /// List all active tabs
    List {
        /// Filter tabs by profile
        #[arg(long)]
        profile: Option<String>,
    },

    /// Close a specific tab
    Close {
        /// Tab name
        name: String,
    },

    /// Close all tabs
    CloseAll,
}

pub async fn handle_tab(command: TabCommands) -> Result<()> {
    // Since we're CLI-only, tabs only exist in the daemon
    if !DaemonClient::is_daemon_running() {
        println!("No daemon running. Start with: webprobe daemon run");
        println!("Tabs only persist with the daemon running.");
        return Ok(());
    }

    match command {
        TabCommands::List { profile } => {
            match DaemonClient::send_request(DaemonRequest::ListTabs {
                profile: profile.clone(),
            }) {
                Ok(DaemonResponse::TabList(tabs)) => {
                    if tabs.is_empty() {
                        println!("No active tabs");
                    } else {
                        println!("Active tabs:");
                        for tab in tabs {
                            let url_str = tab.url.as_deref().unwrap_or("(no URL)");
                            let profile_str = tab
                                .profile
                                .as_deref()
                                .map(|p| format!(" [profile: {}]", p))
                                .unwrap_or_default();
                            println!("  {}{} - {}", tab.name, profile_str, url_str);
                        }
                    }
                }
                Ok(DaemonResponse::Error(e)) => {
                    eprintln!("Error: {}", e);
                }
                Err(e) => {
                    eprintln!("Failed to communicate with daemon: {}", e);
                }
                _ => {}
            }
        }
        TabCommands::Close { name } => {
            match DaemonClient::send_request(DaemonRequest::CloseTab { name: name.clone() }) {
                Ok(DaemonResponse::Success(msg)) => {
                    println!("{}", msg);
                }
                Ok(DaemonResponse::Error(e)) => {
                    eprintln!("Error: {}", e);
                }
                Err(e) => {
                    eprintln!("Failed to communicate with daemon: {}", e);
                }
                _ => {}
            }
        }
        TabCommands::CloseAll => {
            // Send close requests for all tabs
            match DaemonClient::send_request(DaemonRequest::ListTabs { profile: None }) {
                Ok(DaemonResponse::TabList(tabs)) => {
                    let count = tabs.len();
                    for tab in tabs {
                        let _ =
                            DaemonClient::send_request(DaemonRequest::CloseTab { name: tab.name });
                    }
                    println!("Closed {} tab(s)", count);
                }
                _ => {
                    eprintln!("Failed to get tab list from daemon");
                }
            }
        }
    }
    Ok(())
}
