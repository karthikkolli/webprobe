use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::OutputFormat;

pub async fn handle_iframe(
    url: String,
    iframe: String,
    selector: String,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    format: OutputFormat,
    tab: Option<String>,
) -> Result<()> {
    info!("Inspecting {} within iframe {}", selector, iframe);

    // Browser is handled by the daemon

    // Require daemon for all operations
    utils::require_daemon()?;
    // Create the request
    let request = DaemonRequest::Iframe {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url,
        iframe_selector: iframe.clone(),
        element_selector: selector.clone(),
        profile,
    };

    // Send request to daemon
    match DaemonClient::send_request(request) {
        Ok(DaemonResponse::IframeResult(results)) => {
            // Output results
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&results)?);
                }
                OutputFormat::Simple => {
                    println!("Found {} element(s) in iframe {}", results.len(), iframe);
                    for (i, result) in results.iter().enumerate() {
                        if results.len() > 1 {
                            println!("[{}] {}: element", i, result.selector);
                        } else {
                            println!("{}: element", result.selector);
                        }
                        println!("  Position: ({}, {})", result.position.x, result.position.y);
                        println!("  Size: {}x{}", result.size.width, result.size.height);
                        if let Some(text) = &result.text_content {
                            println!("  Text: {}", text);
                        }
                    }
                }
            }
            Ok(())
        }
        Ok(DaemonResponse::Error(e)) => Err(anyhow::anyhow!("Failed to inspect iframe: {}", e)),
        Ok(_) => Err(anyhow::anyhow!("Unexpected response from daemon")),
        Err(e) => Err(anyhow::anyhow!("Failed to communicate with daemon: {}", e)),
    }
}
