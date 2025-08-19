use anyhow::Result;
use tracing::info;

use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

use super::utils::{self, require_daemon};

pub async fn handle_click(
    url: String,
    selector: String,
    index: Option<usize>,
    _browser: String, // Daemon uses its configured browser
    profile: Option<String>,
    _viewport: Option<String>, // Not used in daemon mode
    _no_headless: bool,        // Not used in daemon mode
    tab: Option<String>,
) -> Result<()> {
    info!("Clicking {} on {}", selector, url);

    // Ensure daemon is running (daemon-only architecture)
    require_daemon()?;

    // Resolve tab name based on profile and tab arguments
    let tab_name = utils::resolve_tab_name(&profile, tab)?;
    let is_temp_tab = tab_name.is_empty();

    let request = DaemonRequest::Click {
        tab_name: tab_name.clone(),
        url: if url.is_empty() { None } else { Some(url) },
        selector: selector.clone(),
        index,
        profile,
    };

    let result = match DaemonClient::send_request(request)? {
        DaemonResponse::Success(msg) => {
            println!("{}", msg);

            // Print success message
            if let Some(idx) = index {
                println!(
                    "Successfully clicked element: {} at index {}",
                    selector, idx
                );
            } else {
                println!("Successfully clicked element: {}", selector);
            }
            Ok(())
        }
        DaemonResponse::Error(e) => {
            eprintln!("Error: {}", e);
            Err(anyhow::anyhow!(e))
        }
        _ => {
            eprintln!("Unexpected response from daemon");
            Err(anyhow::anyhow!("Unexpected daemon response"))
        }
    };

    // Clean up temporary tab if it was created
    if is_temp_tab {
        let _ = DaemonClient::send_request(DaemonRequest::CloseTab { name: tab_name });
    }

    result
}
