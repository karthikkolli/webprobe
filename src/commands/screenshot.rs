use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_screenshot(
    url: String,
    selector: Option<String>,
    output: String,
    _browser: String,
    profile: Option<String>,
    _viewport: Option<String>,
    _no_headless: bool,
    tab: Option<String>,
) -> Result<()> {
    info!(
        "Taking screenshot{}",
        if selector.is_some() {
            " of element"
        } else {
            " of page"
        }
    );
    utils::require_daemon()?;

    let request = DaemonRequest::Screenshot {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url,
        selector,
        output: output.clone(),
        profile,
    };

    match DaemonClient::send_request(request) {
        Ok(DaemonResponse::ScreenshotResult { saved_to, bytes }) => {
            println!("Screenshot saved to: {}", saved_to);
            println!("Size: {} bytes", bytes);
            Ok(())
        }
        Ok(DaemonResponse::Error(e)) => Err(anyhow::anyhow!("Failed to take screenshot: {}", e)),
        Ok(_) => Err(anyhow::anyhow!("Unexpected response from daemon")),
        Err(e) => Err(anyhow::anyhow!("Failed to communicate with daemon: {}", e)),
    }
}
