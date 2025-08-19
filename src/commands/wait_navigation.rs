use anyhow::Result;
use tracing::info;

use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_wait_navigation(
    url: String,
    to: Option<String>,
    timeout: u64,
    tab: String,
    _browser: String,
) -> Result<()> {
    info!("Waiting for navigation with timeout {}s", timeout);

    // WaitNavigation requires a tab and daemon
    if !DaemonClient::is_daemon_running() {
        eprintln!("Error: wait-navigation requires the daemon to be running.");
        eprintln!("Start the daemon with: webprobe daemon start");
        return Ok(());
    }

    // Use daemon for wait navigation
    let request = DaemonRequest::WaitNavigation {
        tab_name: tab.clone(),
        url: url.clone(),
        to,
        timeout,
        profile: None,
    };

    match DaemonClient::send_request(request) {
        Ok(DaemonResponse::WaitNavigationResult(new_url)) => {
            println!("✓ Navigation detected: {}", new_url);
        }
        Ok(DaemonResponse::Error(e)) => {
            eprintln!("✗ {}", e);
            return Err(anyhow::anyhow!(e));
        }
        Err(e) => {
            eprintln!("Failed to communicate with daemon: {}", e);
            return Err(e);
        }
        _ => {
            eprintln!("Unexpected response from daemon");
        }
    }
    Ok(())
}
