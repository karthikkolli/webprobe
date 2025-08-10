use anyhow::{Context, Result};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::webdriver::BrowserType;

/// Manages WebDriver processes (geckodriver, chromedriver)
pub struct WebDriverManager {
    processes: Arc<Mutex<Vec<WebDriverProcess>>>,
}

struct WebDriverProcess {
    _browser_type: BrowserType,
    child: Child,
    port: u16,
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
        // First check if driver is already running (either managed by us or externally)
        let url = browser_type.get_webdriver_url();
        if Self::is_driver_running(&url).await {
            debug!("WebDriver already running at {}", url);
            // Try to verify it's actually working by creating a test session
            if Self::verify_driver_working(&url).await {
                // If it's running and working, and we don't have it tracked, it's external
                // Don't try to manage it
                return Ok(url);
            } else {
                info!(
                    "WebDriver at {} is not responding properly, killing and restarting",
                    url
                );
                self.kill_existing_driver(browser_type);
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
                (
                    "geckodriver",
                    vec!["--port".to_string(), port.to_string()],
                    port,
                )
            }
            BrowserType::Chrome => {
                let port = Self::find_free_port_for_browser(browser_type)?;
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

        // Start the process
        let child = Command::new(command)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Failed to start {}", command))?;

        let url = format!("http://localhost:{}", port);

        // Store the process
        {
            let mut processes = self.processes.lock().unwrap();
            processes.push(WebDriverProcess {
                _browser_type: *browser_type,
                child,
                port,
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
                return Ok(port);
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
        let command = match browser_type {
            BrowserType::Firefox => "geckodriver",
            BrowserType::Chrome => "chromedriver",
        };

        #[cfg(unix)]
        {
            let _ = Command::new("pkill").arg("-f").arg(command).output();
        }

        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", &format!("{}.exe", command)])
                .output();
        }

        // Also remove from our managed processes list
        let mut processes = self.processes.lock().unwrap();
        processes.retain(|p| p._browser_type != *browser_type);
    }

    /// Clean up a failed process
    fn cleanup_failed_process(&self, port: u16) {
        let mut processes = self.processes.lock().unwrap();
        if let Some(index) = processes.iter().position(|p| p.port == port) {
            let mut process = processes.remove(index);
            let _ = process.child.kill();
        }
    }

    /// Clean up any orphaned WebDriver processes that aren't on standard ports
    fn cleanup_orphaned_drivers(&self, browser_type: &BrowserType) {
        // Use platform-specific commands to find and kill orphaned processes
        #[cfg(unix)]
        let driver_name = match browser_type {
            BrowserType::Firefox => "geckodriver",
            BrowserType::Chrome => "chromedriver",
        };

        #[cfg(not(unix))]
        let _driver_name = match browser_type {
            BrowserType::Firefox => "geckodriver",
            BrowserType::Chrome => "chromedriver",
        };

        #[cfg(unix)]
        {
            // Find all processes matching the driver name
            if let Ok(output) = Command::new("pgrep").arg("-f").arg(driver_name).output() {
                let pids = String::from_utf8_lossy(&output.stdout);
                for pid_str in pids.lines() {
                    if let Ok(pid) = pid_str.trim().parse::<u32>() {
                        // Check if this PID is one we're tracking
                        let processes = self.processes.lock().unwrap();
                        let is_tracked = processes.iter().any(|p| p.child.id() == pid);
                        drop(processes);

                        if !is_tracked {
                            // This is an orphaned process, kill it
                            debug!("Killing orphaned {} process with PID {}", driver_name, pid);
                            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
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
