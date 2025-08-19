use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::OutputFormat;

pub async fn handle_compare(
    url1: String,
    url2: String,
    mode: String,
    selector: Option<String>,
    _browser: String,
    profile: Option<String>,
    viewport: Option<String>,
    _no_headless: bool,
    format: OutputFormat,
    tab: Option<String>,
) -> Result<()> {
    info!("Comparing {} and {}", url1, url2);

    // Browser and viewport are handled by the daemon
    let _ = viewport; // May be used in future daemon updates

    // Require daemon for all operations
    utils::require_daemon()?;
    // Create the request
    let request = DaemonRequest::Compare {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url1,
        url2,
        mode: mode.clone(),
        selector,
        profile,
    };

    // Send request to daemon
    let comparison = match DaemonClient::send_request(request) {
        Ok(DaemonResponse::CompareResult(result)) => result,
        Ok(DaemonResponse::Error(e)) => {
            return Err(anyhow::anyhow!("Failed to compare: {}", e));
        }
        Ok(_) => {
            return Err(anyhow::anyhow!("Unexpected response from daemon"));
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to communicate with daemon: {}", e));
        }
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&comparison)?);
        }
        OutputFormat::Simple => {
            if let Some(metrics) = comparison.get("metrics") {
                println!("Comparison Results:");
                println!(
                    "Similarity: {:.1}%",
                    metrics
                        .get("similarity_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                );
                println!(
                    "Total differences: {}",
                    metrics
                        .get("total_differences")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "Mode: {}",
                    metrics
                        .get("comparison_mode")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
            }
        }
    }
    Ok(())
}
