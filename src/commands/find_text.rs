use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::OutputFormat;

pub async fn handle_find_text(
    url: String,
    text: String,
    element_type: Option<String>,
    fuzzy: bool,
    case_sensitive: bool,
    all: bool,
    index: Option<usize>,
    tab: Option<String>,
    profile: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    info!("Finding elements with text: {}", text);

    // Require daemon for all operations
    utils::require_daemon()?;

    // Prepare the daemon request - use empty tab_name for one-shot operation
    let request = DaemonRequest::FindText {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url: url.clone(),
        text: text.clone(),
        fuzzy,
        case_sensitive,
        element_type: element_type.clone(),
        all,
        index,
        profile: profile.clone(),
    };

    // Send request to daemon
    let results = match DaemonClient::send_request(request)? {
        DaemonResponse::FindTextResult(results) => results,
        DaemonResponse::Error(e) => {
            return Err(anyhow::anyhow!(e));
        }
        _ => {
            return Err(anyhow::anyhow!("Unexpected response from daemon"));
        }
    };

    // Format and display results
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        OutputFormat::Simple => {
            if results.is_empty() {
                println!("No elements found containing text: {}", text);
            } else {
                for (i, result) in results.iter().enumerate() {
                    if results.len() > 1 {
                        println!("[{}] {}", i, result.tag);
                    } else {
                        println!("{}", result.tag);
                    }
                    println!("  Text: {}", result.text);
                    println!("  Selector: {}", result.selector);
                    if let Some(pos) = result.position.as_object()
                        && let (Some(x), Some(y)) = (pos.get("x"), pos.get("y"))
                    {
                        println!("  Position: ({}, {})", x, y);
                    }
                    if let Some(size) = result.size.as_object()
                        && let (Some(w), Some(h)) = (size.get("width"), size.get("height"))
                    {
                        println!("  Size: {}x{}", w, h);
                    }
                    if !result.visible {
                        println!("  ⚠️  Element is hidden");
                    }
                }
            }
        }
    }
    Ok(())
}
