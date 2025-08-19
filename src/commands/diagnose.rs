use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_diagnose(
    url: String,
    selector: Option<String>,
    check: String,
    _browser: String,
    profile: Option<String>,
    viewport: Option<String>,
    _no_headless: bool,
    tab: Option<String>,
) -> Result<()> {
    info!("Diagnosing layout issues on {}", url);

    // Require daemon for all operations
    utils::require_daemon()?;
    // Create the request
    let request = DaemonRequest::Diagnose {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url,
        selector,
        check_type: check,
        viewport,
        profile,
    };

    // Send request to daemon
    match DaemonClient::send_request(request) {
        Ok(DaemonResponse::DiagnoseResult(diagnosis)) => {
            println!("{}", serde_json::to_string_pretty(&diagnosis)?);
            Ok(())
        }
        Ok(DaemonResponse::Error(e)) => Err(anyhow::anyhow!("Failed to diagnose: {}", e)),
        Ok(_) => Err(anyhow::anyhow!("Unexpected response from daemon")),
        Err(e) => Err(anyhow::anyhow!("Failed to communicate with daemon: {}", e)),
    }
}
