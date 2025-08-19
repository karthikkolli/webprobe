use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_scroll(
    url: String,
    selector: Option<String>,
    by_x: i32,
    by_y: i32,
    to: Option<String>,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    tab: Option<String>,
) -> Result<()> {
    info!("Scrolling on {}", url);

    // Require daemon for all operations
    utils::require_daemon()?;
    let request = DaemonRequest::Scroll {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url: url.clone(),
        selector: selector.clone(),
        by_x,
        by_y,
        to: to.clone(),
        profile,
    };

    match DaemonClient::send_request(request)? {
        DaemonResponse::Success(_) => {
            if let Some(to_pos) = &to {
                println!("Scrolled to position: {}", to_pos);
            } else {
                println!("Scrolled by ({}, {}) pixels", by_x, by_y);
            }
            Ok(())
        }
        DaemonResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
