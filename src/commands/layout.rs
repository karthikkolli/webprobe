use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::OutputFormat;

pub async fn handle_layout(
    url: String,
    selector: String,
    depth: u8,
    max_elements: usize,
    wait_stable: u64,
    detect_shadow: bool,
    profile: Option<String>,
    format: OutputFormat,
    tab: Option<String>,
) -> Result<()> {
    info!("Analyzing layout of {} on {}", selector, url);

    // Require daemon for all operations
    utils::require_daemon()?;
    let request = DaemonRequest::Layout {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url: url.clone(),
        selector: selector.clone(),
        depth,
        max_elements,
        wait_stable,
        detect_shadow,
        profile: profile.clone(),
    };

    match DaemonClient::send_request(request)? {
        DaemonResponse::LayoutResult(layout) => {
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&layout)?);
                }
                OutputFormat::Simple => {
                    println!("Layout Analysis for: {}", layout.selector);
                    println!("Tag: {}, Classes: {:?}", layout.tag, layout.classes);
                    println!("Position: ({}, {})", layout.bounds.x, layout.bounds.y);
                    println!("Size: {}x{}", layout.bounds.width, layout.bounds.height);

                    if !layout.children.is_empty() {
                        println!("\nChildren ({}):", layout.children.len());
                        for (i, child) in layout.children.iter().enumerate() {
                            println!(
                                "  [{}] {} - {}x{} at ({}, {})",
                                i,
                                child.tag,
                                child.bounds.width,
                                child.bounds.height,
                                child.bounds.x,
                                child.bounds.y
                            );
                        }
                    }

                    if !layout.warnings.is_empty() {
                        println!("\nWarnings:");
                        for warning in &layout.warnings {
                            println!("  ⚠️  {}", warning);
                        }
                    }
                }
            }
            Ok(())
        }
        DaemonResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
