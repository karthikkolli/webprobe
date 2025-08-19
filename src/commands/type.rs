use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_type(
    url: String,
    selector: String,
    text: String,
    clear: bool,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    tab: Option<String>,
) -> Result<()> {
    info!("Typing into {} on {}", selector, url);

    // Require daemon for all operations
    utils::require_daemon()?;
    let request = DaemonRequest::Type {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url: if url.is_empty() {
            None
        } else {
            Some(url.clone())
        },
        selector: selector.clone(),
        text: text.clone(),
        clear,
        profile: profile.clone(),
    };

    match DaemonClient::send_request(request)? {
        DaemonResponse::Success(msg) => {
            println!("{}", msg);
            Ok(())
        }
        DaemonResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
