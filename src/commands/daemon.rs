use anyhow::Result;
use clap::Subcommand;

use crate::daemon::{Daemon, DaemonClient, DaemonRequest};
use crate::webdriver::BrowserType;

#[derive(Subcommand)]
pub enum DaemonCommands {
    /// Run the daemon (in foreground)
    Run {
        /// Browser type to use (firefox or chrome)
        #[arg(long, default_value = "chrome")]
        browser: BrowserType,
    },

    /// Start the daemon (show instructions)
    Start {
        /// Browser type to use (firefox or chrome)
        #[arg(long, default_value = "chrome")]
        browser: BrowserType,
    },

    /// Stop the daemon
    Stop,

    /// Check daemon status
    Status,
}

pub async fn handle_daemon(command: DaemonCommands) -> Result<()> {
    match command {
        DaemonCommands::Run { browser } => {
            if Daemon::is_running() {
                println!("Daemon is already running");
            } else {
                println!("Starting daemon with browser: {}...", browser);
                let daemon = Daemon::new(Some(browser)).await?;
                daemon.start().await?;
            }
        }
        DaemonCommands::Start { browser } => {
            if Daemon::is_running() {
                println!("Daemon is already running");
            } else {
                println!("Starting daemon in background...");

                // Get log file path
                let log_dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
                let log_file = log_dir.join("webprobe-daemon.log");

                // Fork and daemonize on Unix
                #[cfg(unix)]
                {
                    use nix::unistd::{ForkResult, fork, setsid};
                    use std::os::unix::io::AsRawFd;
                    use std::os::unix::process::CommandExt;

                    match unsafe { fork() } {
                        Ok(ForkResult::Parent { .. }) => {
                            // Parent process: wait and check if daemon started
                            // Check multiple times with longer timeout for slow starts
                            let mut daemon_started = false;
                            for i in 0..10 {
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                if Daemon::is_running() {
                                    daemon_started = true;
                                    break;
                                }
                                // Give more time for the first attempt
                                if i == 0 {
                                    std::thread::sleep(std::time::Duration::from_millis(1500));
                                }
                            }

                            if daemon_started {
                                println!("Daemon started successfully");
                                println!("Log file: {}", log_file.display());
                            } else {
                                eprintln!(
                                    "Failed to start daemon. Check log file: {}",
                                    log_file.display()
                                );
                            }
                        }
                        Ok(ForkResult::Child) => {
                            // Child process: become a daemon
                            // Create new session
                            let _ = setsid();

                            // Redirect stdout/stderr to log file
                            let log_fd = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(&log_file)?;

                            // Redirect stdout and stderr
                            let log_fd = log_fd.as_raw_fd();
                            nix::unistd::dup2(log_fd, 1)?; // stdout
                            nix::unistd::dup2(log_fd, 2)?; // stderr

                            // Close stdin
                            nix::unistd::close(0)?;

                            // Execute ourselves with daemon run
                            // This creates a fresh process without Tokio runtime issues
                            let exe_path = std::env::current_exe()?;
                            let _ = std::process::Command::new(exe_path)
                                .arg("daemon")
                                .arg("run")
                                .arg("--browser")
                                .arg(browser.to_string())
                                .exec();

                            // If exec fails, exit
                            std::process::exit(1);
                        }
                        Err(e) => {
                            eprintln!("Fork failed: {}", e);
                        }
                    }
                }

                #[cfg(not(unix))]
                {
                    // Windows or other platforms: use simple spawn approach
                    use std::process::Command;
                    let exe_path = std::env::current_exe()?;

                    let child = Command::new(&exe_path)
                        .arg("daemon")
                        .arg("run")
                        .arg("--browser")
                        .arg(browser.to_string())
                        .stdin(std::process::Stdio::null())
                        .stdout(std::fs::File::create(&log_file)?)
                        .stderr(std::fs::File::create(&log_file)?)
                        .spawn()?;

                    std::mem::forget(child);

                    // Check multiple times with longer timeout for slow starts
                    let mut daemon_started = false;
                    for i in 0..10 {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        if Daemon::is_running() {
                            daemon_started = true;
                            break;
                        }
                        // Give more time for the first attempt
                        if i == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                        }
                    }

                    if daemon_started {
                        println!("Daemon started successfully");
                        println!("Log file: {}", log_file.display());
                    } else {
                        eprintln!(
                            "Failed to start daemon. Check log file: {}",
                            log_file.display()
                        );
                    }
                }
            }
        }
        DaemonCommands::Stop => {
            if DaemonClient::is_daemon_running() {
                match DaemonClient::send_request(DaemonRequest::Shutdown) {
                    Ok(_) => println!("Daemon stopped"),
                    Err(e) => println!("Failed to stop daemon: {}", e),
                }
            } else {
                println!("Daemon is not running");
            }
        }
        DaemonCommands::Status => {
            if DaemonClient::is_daemon_running() {
                match DaemonClient::send_request(DaemonRequest::Ping) {
                    Ok(crate::daemon::DaemonResponse::Pong) => {
                        println!("Daemon is running");

                        // List tabs
                        if let Ok(crate::daemon::DaemonResponse::TabList(tabs)) =
                            DaemonClient::send_request(DaemonRequest::ListTabs { profile: None })
                            && !tabs.is_empty()
                        {
                            println!("\nActive tabs:");
                            for tab in tabs {
                                println!(
                                    "  {} - {}",
                                    tab.name,
                                    tab.url.as_deref().unwrap_or("(no URL)")
                                );
                            }
                        }
                    }
                    _ => println!("Daemon is not responding properly"),
                }
            } else {
                println!("Daemon is not running");
            }
        }
    }
    Ok(())
}
