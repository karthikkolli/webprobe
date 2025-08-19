use anyhow::Result;

use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_status(tab: String, _browser: String, profile: Option<String>) -> Result<()> {
    // Status command requires daemon
    if !DaemonClient::is_daemon_running() {
        eprintln!("Error: Status command requires the daemon to be running.");
        eprintln!("Start the daemon with: webprobe daemon start");
        return Ok(());
    }

    // Send status request to daemon
    let request = DaemonRequest::Status {
        tab_name: tab.clone(),
        profile,
    };

    match DaemonClient::send_request(request) {
        Ok(DaemonResponse::StatusResult(status)) => {
            println!("=== Session Status for tab '{}' ===", tab);
            println!("{}", serde_json::to_string_pretty(&status)?);
            println!("=====================================");
        }
        Ok(DaemonResponse::Error(e)) => {
            eprintln!("Error getting status: {}", e);
        }
        Ok(_) => {
            eprintln!("Unexpected response from daemon");
        }
        Err(e) => {
            eprintln!("Error communicating with daemon: {}", e);
        }
    }
    Ok(())
}
