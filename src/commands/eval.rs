use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::OutputFormat;

pub async fn handle_eval(
    url: Option<String>,
    code: String,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    format: OutputFormat,
    tab: Option<String>,
    unsafe_eval: bool,
) -> Result<()> {
    // Security check: require explicit flag
    if !unsafe_eval {
        eprintln!(
            "Error: The eval command requires the --unsafe-eval flag to acknowledge security risks."
        );
        eprintln!("JavaScript execution can be dangerous. Only use with trusted code.");
        eprintln!("Usage: webprobe eval \"your code\" --unsafe-eval");
        std::process::exit(1);
    }

    info!("Executing JavaScript");

    // Require daemon for all operations
    utils::require_daemon()?;
    let request = DaemonRequest::Eval {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url: url.clone(),
        code: code.clone(),
        profile: profile.clone(),
    };

    let result = match DaemonClient::send_request(request)? {
        DaemonResponse::EvalResult(result) => result,
        DaemonResponse::Error(e) => {
            return Err(anyhow::anyhow!(e));
        }
        _ => {
            return Err(anyhow::anyhow!("Unexpected response from daemon"));
        }
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        OutputFormat::Simple => match result {
            serde_json::Value::String(s) => println!("{}", s),
            serde_json::Value::Number(n) => println!("{}", n),
            serde_json::Value::Bool(b) => println!("{}", b),
            serde_json::Value::Null => println!("null"),
            _ => println!("{}", result),
        },
    }
    Ok(())
}
