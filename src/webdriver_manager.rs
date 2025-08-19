use anyhow::{Context, Result};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::webdriver::BrowserType;

/// Manages WebDriver processes (geckodriver, chromedriver)
pub struct WebDriverManager {
    processes: Arc<Mutex<Vec<WebDriverProcess>>>,
}

struct WebDriverProcess {
    browser_type: BrowserType,
    child: Child,
    port: u16,
    url: String,
    #[cfg(unix)]
    process_group_id: Option<i32>,
}

impl Default for WebDriverManager {
    fn default() -> Self {
        Self {
            processes: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl WebDriverManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensure a WebDriver is running for the given browser type
    /// Returns the URL to connect to
    pub async fn ensure_driver(&self, browser_type: &BrowserType) -> Result<String> {
        // First check if we already have a managed driver running for this browser type
        let managed_urls: Vec<String> = {
            let processes = self.processes.lock().unwrap();
            processes
                .iter()
                .filter(|p| p.browser_type == *browser_type)
                .map(|p| p.url.clone())
                .collect()
        };

        for url in managed_urls {
            // We have a managed driver, check if it's still working
            if Self::verify_driver_working(&url).await {
                debug!("Using existing managed WebDriver at {}", url);
                return Ok(url);
            }
        }

        // Check standard ports for externally managed drivers
        let standard_urls = match browser_type {
            BrowserType::Firefox => vec!["http://localhost:4444"],
            BrowserType::Chrome => vec!["http://localhost:9515"],
        };

        for url in standard_urls {
            if Self::is_driver_running(url).await && Self::verify_driver_working(url).await {
                debug!("Found external WebDriver at {}", url);
                return Ok(url.to_string());
            }
        }

        // Not running or not working, try to start it
        info!("WebDriver not detected, attempting to start automatically...");

        // Before starting, make sure we don't have orphaned processes on other ports
        self.cleanup_orphaned_drivers(browser_type);

        self.start_driver(browser_type).await
    }

    /// Start a WebDriver process
    async fn start_driver(&self, browser_type: &BrowserType) -> Result<String> {
        let (command, args, port) = match browser_type {
            BrowserType::Firefox => {
                let port = Self::find_free_port_for_browser(browser_type)?;
                info!("Starting geckodriver on port {}", port);
                (
                    "geckodriver",
                    vec!["--port".to_string(), port.to_string()],
                    port,
                )
            }
            BrowserType::Chrome => {
                let port = Self::find_free_port_for_browser(browser_type)?;
                info!("Starting chromedriver on port {}", port);
                ("chromedriver", vec![format!("--port={}", port)], port)
            }
        };

        // Check if command exists in PATH
        if !Self::command_exists(command) {
            anyhow::bail!(
                "{} not found in PATH. Please install it:\n\
                  macOS: brew install {}\n\
                  Linux: Download from official releases\n\
                  Or see: https://www.selenium.dev/documentation/webdriver/getting_started/install_drivers/",
                command,
                command
            );
        }

        // Build the command
        let mut cmd = Command::new(command);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // On Unix, create a new process group so we can kill the entire tree
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0); // Create new process group with pgid = pid
        }

        // Start the process
        let child = cmd
            .spawn()
            .context(format!("Failed to start {}", command))?;

        // Get the process group ID (on Unix, it's the same as PID when we use process_group(0))
        #[cfg(unix)]
        let process_group_id = Some(child.id() as i32);

        let url = format!("http://localhost:{}", port);

        // Store the process
        {
            let mut processes = self.processes.lock().unwrap();
            processes.push(WebDriverProcess {
                browser_type: *browser_type,
                child,
                port,
                url: url.clone(),
                #[cfg(unix)]
                process_group_id,
            });
        }

        // Wait for driver to be ready (with timeout)
        let max_attempts = 30; // 3 seconds total
        for attempt in 1..=max_attempts {
            if Self::is_driver_running(&url).await {
                info!("WebDriver started successfully on port {}", port);
                return Ok(url);
            }
            if attempt < max_attempts {
                sleep(Duration::from_millis(100)).await;
            }
        }

        // If we get here, driver didn't start properly
        self.cleanup_failed_process(port);
        anyhow::bail!("WebDriver failed to start within timeout")
    }

    /// Check if a command exists in PATH
    pub fn command_exists(command: &str) -> bool {
        #[cfg(unix)]
        {
            Command::new("which")
                .arg(command)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }

        #[cfg(windows)]
        {
            Command::new("where")
                .arg(command)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        }
    }

    /// Find a free port to use
    pub fn find_free_port_for_browser(browser_type: &BrowserType) -> Result<u16> {
        // Try browser-specific ports first to avoid conflicts
        let preferred_ports = match browser_type {
            BrowserType::Firefox => vec![4444, 4445, 4446], // Firefox/geckodriver ports
            BrowserType::Chrome => vec![9515, 9516, 9517],  // Chrome/chromedriver ports
        };

        for port in preferred_ports {
            if !Self::is_port_in_use(port) {
                debug!("Found free port {} for {:?}", port, browser_type);
                return Ok(port);
            } else {
                debug!("Port {} is in use for {:?}", port, browser_type);
            }
        }

        // Fall back to letting OS assign a port
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    /// Check if a port is in use
    pub fn is_port_in_use(port: u16) -> bool {
        std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
    }

    /// Check if WebDriver is running at the given URL
    pub async fn is_driver_running(url: &str) -> bool {
        let status_url = format!("{}/status", url);

        match reqwest::Client::new()
            .get(&status_url)
            .timeout(Duration::from_secs(1))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Verify that WebDriver is actually working (not just running)
    async fn verify_driver_working(url: &str) -> bool {
        // Try to get status - a working driver should return ready:true
        let status_url = format!("{}/status", url);

        match reqwest::Client::new()
            .get(&status_url)
            .timeout(Duration::from_secs(1))
            .send()
            .await
        {
            Ok(response) => {
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    // Check if the driver reports it's ready
                    body.get("value")
                        .and_then(|v| v.get("ready"))
                        .and_then(|r| r.as_bool())
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// Kill existing WebDriver process for a browser type (public method)
    pub fn kill_driver(&self, browser_type: &BrowserType) {
        self.kill_existing_driver(browser_type);
    }

    /// Kill existing WebDriver process for a browser type (internal)
    fn kill_existing_driver(&self, browser_type: &BrowserType) {
        // First, try to kill any managed processes with process groups
        {
            let mut processes = self.processes.lock().unwrap();
            let indices_to_remove: Vec<_> = processes
                .iter()
                .enumerate()
                .filter(|(_, p)| p.browser_type == *browser_type)
                .map(|(i, _)| i)
                .collect();

            // Remove in reverse order to maintain indices
            for index in indices_to_remove.into_iter().rev() {
                let mut process = processes.remove(index);

                // On Unix, kill the entire process group
                #[cfg(unix)]
                if let Some(pgid) = process.process_group_id {
                    info!(
                        "Killing process group {} for {}",
                        pgid,
                        match browser_type {
                            BrowserType::Firefox => "geckodriver",
                            BrowserType::Chrome => "chromedriver",
                        }
                    );
                    Self::kill_process_group(pgid);
                }

                // Also try to kill the child process directly
                let _ = process.child.kill();
            }
        }

        // Then kill any remaining unmanaged processes
        let (driver_command, browser_command) = match browser_type {
            BrowserType::Firefox => ("geckodriver", "firefox"),
            BrowserType::Chrome => ("chromedriver", "chrome"),
        };

        #[cfg(unix)]
        {
            // Kill the WebDriver process
            let _ = Command::new("pkill").arg("-f").arg(driver_command).output();

            // For Firefox, also kill any remaining browser processes
            // This catches any that weren't killed by the process group
            if *browser_type == BrowserType::Firefox {
                warn!("Cleaning up any remaining Firefox processes...");
                let _ = Command::new("pkill")
                    .arg("-f")
                    .arg(browser_command)
                    .output();
            }
        }

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", &format!("{}.exe", driver_command)])
                .output();

            if *browser_type == BrowserType::Firefox {
                let _ = Command::new("taskkill")
                    .args(["/F", "/IM", "firefox.exe"])
                    .output();
            }
        }
    }

    /// Kill a process group on Unix systems
    #[cfg(unix)]
    fn kill_process_group(pgid: i32) {
        use std::process::Command;

        // First try SIGTERM for graceful shutdown
        if let Err(e) = Command::new("kill")
            .args(["-TERM", &format!("-{}", pgid)])
            .output()
        {
            debug!("Failed to send SIGTERM to process group {}: {}", pgid, e);
        }

        // Give processes a moment to shut down gracefully
        std::thread::sleep(Duration::from_millis(100));

        // Then force kill any remaining processes
        if let Err(e) = Command::new("kill")
            .args(["-KILL", &format!("-{}", pgid)])
            .output()
        {
            debug!("Failed to send SIGKILL to process group {}: {}", pgid, e);
        }
    }

    /// Clean up a failed process
    fn cleanup_failed_process(&self, port: u16) {
        let mut processes = self.processes.lock().unwrap();
        if let Some(index) = processes.iter().position(|p| p.port == port) {
            let mut process = processes.remove(index);

            // On Unix, kill the entire process group
            #[cfg(unix)]
            if let Some(pgid) = process.process_group_id {
                info!(
                    "Killing process group {} for failed WebDriver on port {}",
                    pgid, port
                );
                Self::kill_process_group(pgid);
            }

            // Also try to kill the child process directly (fallback for non-Unix)
            let _ = process.child.kill();
        }
    }

    /// Clean up any orphaned WebDriver processes that aren't on standard ports
    fn cleanup_orphaned_drivers(&self, browser_type: &BrowserType) {
        // Use platform-specific commands to find and kill orphaned processes
        #[cfg(unix)]
        let (driver_name, browser_name) = match browser_type {
            BrowserType::Firefox => ("geckodriver", "firefox"),
            BrowserType::Chrome => ("chromedriver", "chrome"),
        };

        #[cfg(not(unix))]
        let _driver_name = match browser_type {
            BrowserType::Firefox => "geckodriver",
            BrowserType::Chrome => "chromedriver",
        };

        #[cfg(unix)]
        {
            // Find all geckodriver/chromedriver processes
            if let Ok(output) = Command::new("pgrep").arg("-f").arg(driver_name).output() {
                let pids = String::from_utf8_lossy(&output.stdout);
                for pid_str in pids.lines() {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        // Check if this PID is one we're tracking
                        let processes = self.processes.lock().unwrap();
                        let is_tracked = processes.iter().any(|p| p.child.id() == pid as u32);
                        drop(processes);

                        if !is_tracked {
                            // This is an orphaned process
                            // Try to get its process group and kill the entire group
                            debug!("Found orphaned {} process with PID {}", driver_name, pid);

                            // Get the process group ID (usually same as PID for group leaders)
                            if let Ok(pgid_output) = Command::new("ps")
                                .args(["-o", "pgid=", "-p", &pid.to_string()])
                                .output()
                            {
                                if let Ok(pgid) = String::from_utf8_lossy(&pgid_output.stdout)
                                    .trim()
                                    .parse::<i32>()
                                {
                                    info!(
                                        "Killing orphaned process group {} for {}",
                                        pgid, driver_name
                                    );
                                    Self::kill_process_group(pgid);
                                } else {
                                    // Fallback: just kill the process directly
                                    let _ = Command::new("kill")
                                        .arg("-9")
                                        .arg(pid.to_string())
                                        .status();
                                }
                            } else {
                                // Fallback: just kill the process directly
                                let _ =
                                    Command::new("kill").arg("-9").arg(pid.to_string()).status();
                            }
                        }
                    }
                }
            }

            // Also clean up any orphaned browser processes
            // This is a safety measure for Firefox processes that might be left behind
            if *browser_type == BrowserType::Firefox {
                // Check for Firefox processes without a parent geckodriver
                if let Ok(firefox_output) =
                    Command::new("pgrep").arg("-f").arg(browser_name).output()
                {
                    let firefox_pids = String::from_utf8_lossy(&firefox_output.stdout);

                    // Get list of geckodriver PIDs for comparison
                    let mut geckodriver_pids = Vec::new();
                    if let Ok(gecko_output) =
                        Command::new("pgrep").arg("-f").arg("geckodriver").output()
                    {
                        let gecko_pids = String::from_utf8_lossy(&gecko_output.stdout);
                        for pid_str in gecko_pids.lines() {
                            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                                geckodriver_pids.push(pid);
                            }
                        }
                    }

                    for pid_str in firefox_pids.lines() {
                        if let Ok(pid) = pid_str.trim().parse::<i32>() {
                            // Check if this Firefox has a geckodriver parent
                            if let Ok(ppid_output) = Command::new("ps")
                                .args(["-o", "ppid=", "-p", &pid.to_string()])
                                .output()
                                && let Ok(ppid) = String::from_utf8_lossy(&ppid_output.stdout)
                                    .trim()
                                    .parse::<i32>()
                            {
                                // If parent is not a geckodriver, it's likely orphaned
                                if !geckodriver_pids.contains(&ppid) && ppid != 1 {
                                    debug!("Found potentially orphaned Firefox process {}", pid);
                                    // Don't kill it here - it might be a user's browser
                                    // Only kill if we're certain it's from our webdriver
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Stop all managed WebDriver processes
    pub fn stop_all(&self) {
        let mut processes = self.processes.lock().unwrap();
        for process in processes.iter_mut() {
            debug!("Stopping WebDriver on port {}", process.port);

            // On Unix, kill the entire process group
            #[cfg(unix)]
            if let Some(pgid) = process.process_group_id {
                info!(
                    "Killing process group {} for WebDriver on port {}",
                    pgid, process.port
                );
                Self::kill_process_group(pgid);
            }

            // Also try to kill the child process directly (fallback)
            let _ = process.child.kill();
        }
        processes.clear();
    }
}

impl Drop for WebDriverManager {
    fn drop(&mut self) {
        // Clean up any processes we started
        self.stop_all();
    }
}

// Global WebDriver manager instance
lazy_static::lazy_static! {
    pub static ref GLOBAL_WEBDRIVER_MANAGER: WebDriverManager = WebDriverManager::new();
}

#[cfg(test)]
#[path = "webdriver_manager_test.rs"]
mod webdriver_manager_test;
