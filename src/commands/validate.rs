use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::OutputFormat;

pub async fn handle_validate(
    url: String,
    check: String,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    format: OutputFormat,
    tab: Option<String>,
) -> Result<()> {
    info!("Validating page {}", url);
    utils::require_daemon()?;

    let request = DaemonRequest::Validate {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url,
        check_type: check,
        profile,
    };

    let validation = match DaemonClient::send_request(request) {
        Ok(DaemonResponse::ValidateResult(result)) => result,
        Ok(DaemonResponse::Error(e)) => {
            return Err(anyhow::anyhow!("Failed to validate: {}", e));
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
            println!("{}", serde_json::to_string_pretty(&validation)?);
        }
        OutputFormat::Simple => {
            if let Some(summary) = validation.get("summary") {
                println!("Validation Results:");
                println!(
                    "Score: {}/100",
                    summary.get("score").and_then(|v| v.as_i64()).unwrap_or(0)
                );
                println!(
                    "Accessibility issues: {}",
                    summary
                        .get("accessibility_issues")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "SEO issues: {}",
                    summary
                        .get("seo_issues")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
                println!(
                    "Performance issues: {}",
                    summary
                        .get("performance_issues")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                );
            }
        }
    }
    Ok(())
}
