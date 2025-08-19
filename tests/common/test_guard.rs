/// Robust test guards for daemon and profile management
/// These guards ensure proper cleanup even if tests panic
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Get the browser to use for testing from TEST_BROWSER env var
/// Defaults to "chrome" if not set
pub fn get_test_browser() -> &'static str {
    match std::env::var("TEST_BROWSER") {
        Ok(browser) if browser == "firefox" => "firefox",
        Ok(browser) if browser == "chrome" => "chrome",
        _ => "chrome", // Default to chrome
    }
}

/// Check if daemon is running
fn is_daemon_running() -> bool {
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "status"])
        .output()
        .expect("Failed to check daemon status");

    String::from_utf8_lossy(&output.stdout).contains("Daemon is running")
}

/// Stop daemon and wait for it to fully stop
fn stop_daemon_and_wait() {
    // Send stop command
    let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "stop"])
        .output();

    // Wait for daemon to actually stop (max 5 seconds)
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(500));
        if !is_daemon_running() {
            return;
        }
    }

    // If still running, forcefully kill all webprobe processes
    #[cfg(unix)]
    {
        let _ = Command::new("pkill").args(["-f", "webprobe"]).output();

        // Also kill any lingering WebDriver processes
        let _ = Command::new("pkill").args(["-f", "chromedriver"]).output();
        let _ = Command::new("pkill").args(["-f", "geckodriver"]).output();

        // Kill any lingering browser processes started by tests
        let _ = Command::new("pkill")
            .args(["-f", "Chrome.*--remote-debugging"])
            .output();
        let _ = Command::new("pkill")
            .args(["-f", "firefox.*-marionette"])
            .output();
    }

    std::thread::sleep(Duration::from_millis(500));
}

/// A guard that manages daemon lifecycle for tests
///
/// This guard ensures:
/// 1. No daemon is running when created (fails if one is)
/// 2. Starts a fresh daemon
/// 3. Cleans up daemon, WebDriver, and browser processes on drop
pub struct DaemonTestGuard {
    browser: String,
    profiles: Vec<String>,
    cleanup_complete: Arc<AtomicBool>,
}

impl DaemonTestGuard {
    /// Create a new daemon guard for testing
    ///
    /// # Panics
    /// Panics if a daemon is already running (ensures test isolation)
    pub fn new(browser: &str) -> Self {
        // Ensure no daemon is running (for test isolation)
        if is_daemon_running() {
            panic!(
                "Cannot create DaemonTestGuard: daemon is already running. \
                 This breaks test isolation. Please ensure all tests use DaemonTestGuard \
                 and don't leave daemons running. \
                 \
                 Possible causes: \
                 1. A previous test didn't clean up properly \
                 2. You manually started a daemon outside of tests \
                 3. Another test is running concurrently (use --test-threads=1) \
                 \
                 To fix: Run 'webprobe daemon stop' or 'pkill -f webprobe'"
            );
        }

        // Start fresh daemon
        let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
            .args(["daemon", "start", "--browser", browser])
            .output()
            .expect("Failed to start daemon");

        if !output.status.success() {
            panic!(
                "Failed to start daemon: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Wait for daemon to be ready (with retries)
        for i in 0..20 {
            std::thread::sleep(Duration::from_millis(500));
            if is_daemon_running() {
                println!("Daemon started successfully after {} attempts", i + 1);
                break;
            }
            if i == 19 {
                panic!("Daemon failed to start after 10 seconds");
            }
        }

        Self {
            browser: browser.to_string(),
            profiles: Vec::new(),
            cleanup_complete: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a test profile and track it for cleanup
    pub fn create_profile(&mut self, name: &str) -> &mut Self {
        // Destroy any existing profile with this name first
        let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
            .args(["profile", "destroy", name, "--force"])
            .output();

        // Create the profile
        let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
            .args(["profile", "create", name, "--browser", &self.browser])
            .output()
            .expect("Failed to create profile");

        if !output.status.success() {
            panic!(
                "Failed to create profile '{}': {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        self.profiles.push(name.to_string());
        self
    }

    /// Perform cleanup (can be called manually or will be called on drop)
    fn cleanup(&self) {
        // Only cleanup once
        if self.cleanup_complete.swap(true, Ordering::SeqCst) {
            return;
        }

        println!("DaemonTestGuard: Starting cleanup...");

        // Destroy all profiles we created
        for profile in &self.profiles {
            let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
                .args(["profile", "destroy", profile, "--force"])
                .output();
        }

        // Stop daemon (this should trigger browser/WebDriver cleanup)
        stop_daemon_and_wait();

        // Extra cleanup for any orphaned processes
        // The daemon should clean up WebDriver and browser processes,
        // but we do extra cleanup to be absolutely sure
        #[cfg(unix)]
        {
            // Kill any WebDriver processes that might be lingering
            let _ = Command::new("pkill").args(["-f", "chromedriver"]).output();
            let _ = Command::new("pkill").args(["-f", "geckodriver"]).output();

            // Kill any browser processes with automation flags
            let _ = Command::new("pkill")
                .args(["-f", "Chrome.*--remote-debugging"])
                .output();
            let _ = Command::new("pkill")
                .args(["-f", "firefox.*-marionette"])
                .output();
        }

        println!("DaemonTestGuard: Cleanup complete");
    }
}

impl Drop for DaemonTestGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// A guard for managing test profiles without managing daemon
/// Use this when you want to manage profiles but not the daemon lifecycle
pub struct ProfileTestGuard {
    profiles: Vec<String>,
}

impl ProfileTestGuard {
    pub fn new() -> Self {
        Self {
            profiles: Vec::new(),
        }
    }

    pub fn create_profile(&mut self, name: &str, browser: &str) -> &mut Self {
        // Destroy any existing profile with this name first
        let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
            .args(["profile", "destroy", name, "--force"])
            .output();

        // Create the profile
        let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
            .args(["profile", "create", name, "--browser", browser])
            .output()
            .expect("Failed to create profile");

        if !output.status.success() {
            panic!(
                "Failed to create profile '{}': {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            );
        }

        self.profiles.push(name.to_string());
        self
    }
}

impl Drop for ProfileTestGuard {
    fn drop(&mut self) {
        // Clean up all profiles we created
        for profile in &self.profiles {
            let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
                .args(["profile", "destroy", profile, "--force"])
                .output();
        }
    }
}
