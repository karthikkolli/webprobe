use anyhow::{Context, Result};
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};

pub async fn handle_batch(
    commands: String,
    tab: Option<String>,
    _browser: String,
    _stop_on_error: bool,
    _headless: bool,
    profile: Option<String>,
    _viewport: Option<String>,
) -> Result<()> {
    info!("Executing batch commands");

    // Require daemon for all operations
    utils::require_daemon()?;
    // Parse commands if needed (for validation purposes)
    let commands_to_send = if let Some(file_path) = commands.strip_prefix('@') {
        // Read from file if starts with @

        std::fs::read_to_string(file_path)
            .context(format!("Failed to read commands from file: {}", file_path))?
    } else {
        commands.clone()
    };

    let request = DaemonRequest::Batch {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        commands: commands_to_send,
        profile: profile.clone(),
    };

    match DaemonClient::send_request(request)? {
        DaemonResponse::BatchResult(results) => {
            println!("\nBatch execution complete:");
            let successes = results.iter().filter(|r| !r["error"].is_string()).count();
            let failures = results.len() - successes;
            println!("  ✓ {} commands succeeded", successes);
            if failures > 0 {
                println!("  ✗ {} commands failed", failures);
            }
            Ok(())
        }
        DaemonResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
