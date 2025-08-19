use crate::commands::utils;
use anyhow::Result;
use tracing::info;

use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::{InspectionDepth, OutputFormat};

#[allow(clippy::too_many_arguments)]
pub async fn handle_inspect(
    url: String,
    selector: String,
    profile: Option<String>,
    format: OutputFormat,
    _depth: InspectionDepth,
    all: bool,
    index: Option<usize>,
    expect_one: bool,
    _viewport: Option<String>,
    tab: Option<String>,
    _console: bool,
) -> Result<()> {
    // Don't log browser type here as it may be different when using daemon
    info!("Inspecting {} on {}", selector, url);

    // Check if daemon is running
    if DaemonClient::is_daemon_running() {
        // Resolve tab name based on profile and tab arguments
        let tab_name = utils::resolve_tab_name(&profile, tab)?;

        // Validate: empty URL only makes sense with an existing tab
        if url.is_empty() && tab_name.is_empty() {
            return Err(anyhow::anyhow!("URL is required for one-shot operations"));
        }

        // Browser is handled by the daemon
        let request = DaemonRequest::Inspect {
            tab_name: tab_name.clone(),
            url: url.clone(),
            selector: selector.clone(),
            all,
            index,
            expect_one,
            profile: profile.clone(),
        };

        match DaemonClient::send_request(request) {
            Ok(DaemonResponse::InspectResult(results, logs)) => {
                match format {
                    OutputFormat::Json => {
                        if results.len() == 1 && !all {
                            println!("{}", serde_json::to_string_pretty(&results[0])?);
                        } else {
                            println!("{}", serde_json::to_string_pretty(&results)?);
                        }
                    }
                    OutputFormat::Simple => {
                        for (i, result) in results.iter().enumerate() {
                            if results.len() > 1 {
                                println!(
                                    "[{}] {}: {} element at ({}, {}) {}x{}px",
                                    i,
                                    result.selector,
                                    result
                                        .computed_styles
                                        .get("tag")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown"),
                                    result.position.x,
                                    result.position.y,
                                    result.size.width,
                                    result.size.height
                                );
                            } else {
                                println!(
                                    "{}: {} element at ({}, {}) {}x{}px",
                                    result.selector,
                                    result
                                        .computed_styles
                                        .get("tag")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown"),
                                    result.position.x,
                                    result.position.y,
                                    result.size.width,
                                    result.size.height
                                );
                            }
                            if let Some(text) = &result.text_content {
                                println!("  Text: {}", text);
                            }
                            if result.children_count > 0 {
                                println!("  Children: {}", result.children_count);
                            }
                        }
                    }
                }

                // Display console logs if captured
                if let Some(logs) = logs
                    && !logs.is_empty()
                {
                    eprintln!("\n=== Console Logs ===");
                    for log in logs {
                        eprintln!("[{}] {}: {}", log.timestamp, log.level, log.message);
                    }
                }

                Ok(())
            }
            Ok(DaemonResponse::Error(e)) => {
                eprintln!("Error: {}", e);
                // Return an error so the command exits with non-zero code
                Err(anyhow::anyhow!("{}", e))
            }
            Err(e) => {
                eprintln!("Failed to communicate with daemon: {}", e);
                Err(anyhow::anyhow!("Failed to communicate with daemon: {}", e))
            }
            _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
        }
    } else {
        // Daemon is not running
        Err(anyhow::anyhow!(
            "Daemon is not running. Start it with: webprobe daemon start"
        ))
    }
}
