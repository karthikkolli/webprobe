use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_detect(
    url: String,
    context: Option<String>,
    tab: Option<String>,
    _browser: String,
    profile: Option<String>,
) -> Result<()> {
    info!("Detecting smart elements on {}", url);
    utils::require_daemon()?;

    let request = DaemonRequest::Detect {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url,
        context,
        profile,
    };

    match DaemonClient::send_request(request)? {
        DaemonResponse::DetectResult(result) => {
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(())
        }
        DaemonResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
