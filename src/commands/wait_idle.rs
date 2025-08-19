use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_wait_idle(
    url: String,
    timeout: u64,
    idle_time: u64,
    tab: Option<String>,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    show_log: bool,
) -> Result<()> {
    info!("Waiting for network idle with timeout {}ms", timeout);
    utils::require_daemon()?;

    let request = DaemonRequest::WaitIdle {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url,
        timeout,
        idle_time,
        profile,
    };

    match DaemonClient::send_request(request)? {
        DaemonResponse::WaitIdleResult(log) => {
            println!("✅ Network became idle");
            if show_log && !log.is_empty() {
                println!("\nNetwork Log:");
                for entry in log {
                    println!("  {}", entry);
                }
            }
            Ok(())
        }
        DaemonResponse::Error(e) => {
            if e.contains("timeout") {
                println!("⏱️  Timeout reached - network still active");
                Ok(())
            } else {
                Err(anyhow::anyhow!(e))
            }
        }
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
