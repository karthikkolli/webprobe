use crate::daemon::DaemonClient;
use anyhow::Result;

/// Require daemon to be running for all operations (daemon-only architecture)
pub fn require_daemon() -> Result<()> {
    if !DaemonClient::is_daemon_running() {
        eprintln!("Error: The daemon is not running.");
        eprintln!("Start the daemon with: webprobe daemon start --browser chrome");
        eprintln!(
            "\nAll webprobe commands require the daemon for consistent performance and state management."
        );
        return Err(anyhow::anyhow!("Daemon not running"));
    }
    Ok(())
}

/// Determine tab name based on profile and tab arguments
/// Returns the tab name to use (empty string for one-shot operations)
pub fn resolve_tab_name(profile: &Option<String>, tab: Option<String>) -> Result<String> {
    // Validate: --tab requires --profile
    if tab.is_some() && profile.is_none() {
        return Err(anyhow::anyhow!("--tab requires --profile to be specified"));
    }

    // Determine tab name based on profile presence
    Ok(match (profile, tab) {
        (Some(_), Some(t)) => t,               // Profile + explicit tab
        (Some(_), None) => "main".to_string(), // Profile without tab = use default "main"
        (None, None) => String::new(),         // No profile = one-shot (empty string)
        (None, Some(_)) => unreachable!(),     // Already handled above
    })
}
