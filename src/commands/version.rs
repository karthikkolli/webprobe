use anyhow::Result;

pub async fn handle_version() -> Result<()> {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const NAME: &str = env!("CARGO_PKG_NAME");
    const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

    println!("{} v{}", NAME, VERSION);
    println!("By: {}", AUTHORS);
    println!("Repository: https://github.com/karthikkolli/webprobe");

    // Check if running from a package manager
    if std::env::var("HOMEBREW_PREFIX").is_ok() {
        println!("Installed via: Homebrew");
    } else if std::path::Path::new("/usr/bin/apt").exists()
        && std::path::Path::new("/usr/share/doc/webprobe").exists()
    {
        println!("Installed via: APT (.deb)");
    } else if std::path::Path::new("/usr/bin/dnf").exists()
        || std::path::Path::new("/usr/bin/yum").exists()
    {
        println!("Installed via: RPM");
    }
    Ok(())
}
