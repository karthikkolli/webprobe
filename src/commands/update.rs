use anyhow::Result;

pub async fn handle_update(install: bool) -> Result<()> {
    check_for_updates(install).await
}

async fn check_for_updates(auto_install: bool) -> Result<()> {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const REPO: &str = "karthikkolli/webprobe";

    println!("Current version: v{}", CURRENT_VERSION);
    println!("Checking for updates...");

    // Fetch latest release from GitHub API
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);

    let response = client
        .get(&url)
        .header("User-Agent", "webprobe-updater")
        .send()
        .await?;

    if !response.status().is_success() {
        println!("Failed to check for updates: {}", response.status());
        return Ok(());
    }

    let release: serde_json::Value = response.json().await?;
    let latest_version = release["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('v');

    if latest_version == CURRENT_VERSION {
        println!("✅ You are running the latest version!");
        return Ok(());
    }

    println!("🆕 New version available: v{}", latest_version);
    println!(
        "Release notes: {}",
        release["html_url"].as_str().unwrap_or("")
    );

    // Detect installation method
    let update_cmd = if std::env::var("HOMEBREW_PREFIX").is_ok() {
        Some("brew upgrade webprobe")
    } else if std::path::Path::new("/usr/bin/apt").exists()
        && std::path::Path::new("/usr/share/doc/webprobe").exists()
    {
        Some("sudo apt update && sudo apt upgrade webprobe")
    } else if std::path::Path::new("/usr/bin/dnf").exists() {
        Some("sudo dnf upgrade webprobe")
    } else if std::path::Path::new("/usr/bin/yum").exists() {
        Some("sudo yum update webprobe")
    } else if std::path::Path::new("/usr/local/bin/webprobe").exists() {
        Some(
            "curl -fsSL https://raw.githubusercontent.com/karthikkolli/webprobe/main/install.sh | bash",
        )
    } else {
        None
    };

    if let Some(cmd) = update_cmd {
        println!("\nTo update, run:");
        println!("  {}", cmd);

        if auto_install {
            println!("\nAttempting automatic update...");
            let shell = if cfg!(target_os = "windows") {
                "cmd"
            } else {
                "sh"
            };
            let flag = if cfg!(target_os = "windows") {
                "/C"
            } else {
                "-c"
            };

            let status = std::process::Command::new(shell)
                .arg(flag)
                .arg(cmd)
                .status()?;

            if status.success() {
                println!("✅ Update completed successfully!");
                println!("Please restart webprobe to use the new version.");
            } else {
                println!("❌ Automatic update failed. Please run the update command manually.");
            }
        }
    } else {
        println!("\nTo update manually:");
        println!(
            "  1. Download from: https://github.com/{}/releases/latest",
            REPO
        );
        println!("  2. Or reinstall using: cargo install webprobe");
    }

    Ok(())
}
