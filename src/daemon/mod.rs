use crate::browser_manager::BrowserManager;
use crate::types::{CoreProfile, ElementInfo, InspectionDepth, LayoutInfo, Profile, ViewportSize};
use crate::webdriver::{BrowserType, ConsoleMessage};
use anyhow::{Context, Result};
use interprocess::local_socket::{
    GenericFilePath, Listener, ListenerOptions, Name, Stream, ToFsName,
    traits::{ListenerExt, Stream as StreamTrait},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Daemon that runs in the background and maintains browser profiles
pub struct Daemon {
    _auth_token: String,
    browser_type: BrowserType,

    // All profile states (including default and temporary)
    profiles: HashMap<String, ProfileState>,

    // Profile registry for access control and persistence
    profile_registry: HashMap<String, ProfileMetadata>,

    // Track last cleanup time for TTL management
    last_cleanup: chrono::DateTime<chrono::Utc>,

    // Counter for generating unique temporary profile names
    temp_profile_counter: u64,
}

/// Messages that can be sent to the daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonRequest {
    // Authentication (must be first message)
    Authenticate {
        token: String,
    },

    // Tab management
    CreateTab {
        name: String,
        browser_type: String,
        profile: Option<String>,
        viewport: Option<String>,
        headless: bool,
    },
    CloseTab {
        name: String,
    },
    ListTabs {
        profile: Option<String>,
    },

    // Browser operations
    Inspect {
        tab_name: String,
        url: String,
        selector: String,
        all: bool,
        index: Option<usize>,
        expect_one: bool,
        profile: Option<String>,
    },
    Type {
        tab_name: String,
        url: Option<String>,
        selector: String,
        text: String,
        clear: bool,
        profile: Option<String>,
    },
    Click {
        tab_name: String,
        url: Option<String>,
        selector: String,
        index: Option<usize>,
        profile: Option<String>,
    },
    Scroll {
        tab_name: String,
        url: String,
        selector: Option<String>,
        by_x: i32,
        by_y: i32,
        to: Option<String>,
        profile: Option<String>,
    },
    Analyze {
        tab_name: String,
        url: String,
        selector: String,
        focus: String,
        proximity: Option<u32>,
        index: Option<usize>,
        profile: Option<String>,
    },
    Layout {
        tab_name: String,
        url: String,
        selector: String,
        depth: u8,
        max_elements: usize,
        wait_stable: u64,
        detect_shadow: bool,
        profile: Option<String>,
    },
    Wait {
        tab_name: String,
        url: String,
        selector: String,
        timeout: u64,
        condition: String,
        profile: Option<String>,
    },
    Html {
        tab_name: String,
        url: String,
        selector: Option<String>,
        profile: Option<String>,
    },
    Eval {
        tab_name: String,
        url: Option<String>,
        code: String,
        profile: Option<String>,
    },
    Detect {
        tab_name: String,
        url: String,
        context: Option<String>,
        profile: Option<String>,
    },
    FindText {
        tab_name: String,
        url: String,
        text: String,
        fuzzy: bool,
        case_sensitive: bool,
        element_type: Option<String>,
        all: bool,
        index: Option<usize>,
        profile: Option<String>,
    },
    WaitIdle {
        tab_name: String,
        url: String,
        timeout: u64,
        idle_time: u64,
        profile: Option<String>,
    },
    WaitNavigation {
        tab_name: String,
        url: String,
        to: Option<String>,
        timeout: u64,
        profile: Option<String>,
    },
    Status {
        tab_name: String,
        profile: Option<String>,
    },
    Batch {
        tab_name: String,
        commands: String,
        profile: Option<String>,
    },
    Screenshot {
        tab_name: String,
        url: String,
        selector: Option<String>,
        output: String,
        profile: Option<String>,
    },
    Iframe {
        tab_name: String,
        url: String,
        iframe_selector: String,
        element_selector: String,
        profile: Option<String>,
    },
    Diagnose {
        tab_name: String,
        url: String,
        selector: Option<String>,
        check_type: String,
        viewport: Option<String>,
        profile: Option<String>,
    },
    Validate {
        tab_name: String,
        url: String,
        check_type: String,
        profile: Option<String>,
    },
    Compare {
        tab_name: String,
        url1: String,
        url2: String,
        mode: String,
        selector: Option<String>,
        profile: Option<String>,
    },

    // Profile management
    CreateProfile {
        name: String,
        config: ProfileConfig,
    },
    DestroyProfile {
        name: String,
        force: bool,
    },
    ListProfiles,
    ProfileInfo {
        name: String,
    },
    LockProfile {
        name: String,
        duration_minutes: u64,
    },
    UnlockProfile {
        name: String,
    },

    // Daemon control
    Ping,
    Shutdown,
}

#[allow(clippy::large_enum_variant)]
/// Responses from the daemon
#[derive(Debug, Serialize, Deserialize)]
pub enum DaemonResponse {
    Authenticated,
    AuthRequired,
    Success(String),
    Error(String),
    TabList(Vec<TabInfo>),
    InspectResult(Vec<ElementInfo>, Option<Vec<ConsoleMessage>>),
    AnalyzeResult(serde_json::Value),
    LayoutResult(LayoutInfo),
    WaitResult(bool),
    HtmlResult(String),
    EvalResult(serde_json::Value),
    DetectResult(serde_json::Value),
    FindTextResult(Vec<crate::types::TextSearchResult>),
    WaitIdleResult(Vec<String>),
    WaitNavigationResult(String),
    StatusResult(serde_json::Value),
    BatchResult(Vec<serde_json::Value>),
    ScreenshotResult { saved_to: String, bytes: usize },
    IframeResult(Vec<ElementInfo>),
    DiagnoseResult(serde_json::Value),
    ValidateResult(serde_json::Value),
    CompareResult(serde_json::Value),
    ProfileList(Vec<ProfileMetadata>),
    ProfileMetadata(ProfileMetadata),
    Pong,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TabInfo {
    pub name: String,
    pub url: Option<String>,
    pub profile: Option<String>,
    pub browser_type: String,
    /// Viewport size for this tab
    /// NOTE: Requires Chrome DevTools Protocol (CDP) or Firefox Remote Protocol
    /// to set per-tab viewport. Currently not implemented - all tabs share
    /// the browser window's viewport size.
    pub viewport: Option<ViewportSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub browser_type: BrowserType,
    pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
    pub viewport: Option<ViewportSize>,
    pub last_accessed: Option<chrono::DateTime<chrono::Utc>>,
    pub tabs_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub browser_type: BrowserType,
    pub viewport: Option<ViewportSize>,
    pub headless: bool,
    pub persist_cookies: bool,
    pub persist_storage: bool,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            browser_type: BrowserType::Chrome,
            viewport: None,
            headless: true,
            persist_cookies: true,
            persist_storage: true,
        }
    }
}

/// Represents the complete state of a profile including browser, tabs, and storage
#[derive(Debug)]
pub struct ProfileState {
    /// The browser manager for this profile
    pub browser: BrowserManager,
    /// Currently active tab name
    pub active_tab: String,
    /// Metadata for all tabs in this profile
    pub tabs: HashMap<String, TabMetadata>,
    /// When this profile was last accessed
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    /// Whether this is a temporary profile (should be cleaned up after use)
    pub is_temporary: bool,
}

impl ProfileState {
    /// Create a new profile state
    pub async fn new(config: &ProfileConfig, profile_name: Option<String>) -> Result<Self> {
        let browser = BrowserManager::new(
            config.browser_type,
            profile_name,
            config.viewport,
            config.headless,
        )
        .await?;

        Ok(Self {
            browser,
            active_tab: "main".to_string(),
            tabs: HashMap::new(),
            last_accessed: chrono::Utc::now(),
            is_temporary: false,
        })
    }

    /// Create a temporary profile state that will be cleaned up after use
    pub async fn new_temporary(browser_type: BrowserType) -> Result<Self> {
        let config = ProfileConfig {
            browser_type,
            headless: true,
            persist_cookies: false,
            persist_storage: false,
            ..Default::default()
        };

        let mut state = Self::new(&config, None).await?;
        state.is_temporary = true;
        Ok(state)
    }
}

/// Simple tab metadata (without full TabInfo)
#[derive(Debug, Clone)]
pub struct TabMetadata {
    pub url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Daemon {
    /// Get the default browser (compatibility layer)
    fn default_browser(&mut self) -> &mut BrowserManager {
        &mut self
            .profiles
            .get_mut("default")
            .expect("default profile should always exist")
            .browser
    }

    /// Get the oneshot browser (compatibility layer)
    fn oneshot_browser(&mut self) -> &mut BrowserManager {
        // We always create the oneshot profile in new(), so it should always exist
        &mut self
            .profiles
            .get_mut("oneshot")
            .expect("oneshot profile should always exist")
            .browser
    }

    /// Check if custom browser exists (compatibility layer)
    fn custom_browsers_contains(&self, name: &str) -> bool {
        self.profiles.contains_key(name) && name != "default" && name != "oneshot"
    }

    /// Get or create custom browser (compatibility layer)
    async fn get_or_create_custom_browser(&mut self, name: &str) -> Result<&mut BrowserManager> {
        if name == "default" || name == "oneshot" {
            return Err(anyhow::anyhow!("Cannot use reserved profile names"));
        }

        let profile = self.get_or_create_profile(name.to_string()).await?;
        Ok(&mut profile.browser)
    }

    pub async fn new(browser_type: Option<BrowserType>) -> Result<Self> {
        // Use Chrome as default browser if not specified
        let browser_type = browser_type.unwrap_or(BrowserType::Chrome);

        // Generate a random auth token
        let auth_token = Self::generate_auth_token();

        // Save the token to a file that only the current user can read
        Self::save_auth_token(&auth_token)?;

        // Save the browser type for this daemon session
        Self::save_browser_type(&browser_type)?;

        info!("Starting daemon with browser: {:?}", browser_type);

        // Load existing profile registry from disk
        let profile_registry = Self::load_profile_registry().unwrap_or_default();

        // Create the default profile
        info!("Initializing default profile...");
        let default_config = ProfileConfig {
            browser_type,
            headless: true,
            ..Default::default()
        };

        let default_profile = ProfileState::new(&default_config, Some("default".to_string()))
            .await
            .context("Failed to create default profile")?;

        // Create the oneshot profile for temporary operations
        info!("Initializing oneshot profile...");
        let oneshot_config = ProfileConfig {
            browser_type,
            headless: true,
            persist_cookies: false,
            persist_storage: false,
            ..Default::default()
        };

        let oneshot_profile = ProfileState::new(&oneshot_config, Some("oneshot".to_string()))
            .await
            .context("Failed to create oneshot profile")?;

        let mut profiles = HashMap::new();
        profiles.insert("default".to_string(), default_profile);
        profiles.insert("oneshot".to_string(), oneshot_profile);

        Ok(Self {
            _auth_token: auth_token,
            browser_type,
            profiles,
            profile_registry,
            last_cleanup: chrono::Utc::now(),
            temp_profile_counter: 0,
        })
    }

    /// Get the path to the profile registry file
    fn profile_registry_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("webprobe")
            .join("profiles")
            .join("registry.json")
    }

    /// Load profile registry from disk
    fn load_profile_registry() -> Result<HashMap<String, ProfileMetadata>> {
        let path = Self::profile_registry_path();
        if !path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&path).context("Failed to read profile registry")?;

        let registry: HashMap<String, ProfileMetadata> =
            serde_json::from_str(&content).context("Failed to parse profile registry")?;

        Ok(registry)
    }

    /// Save profile registry to disk
    fn save_profile_registry(&self) -> Result<()> {
        let path = Self::profile_registry_path();

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create profile directory")?;
        }

        let content = serde_json::to_string_pretty(&self.profile_registry)
            .context("Failed to serialize profile registry")?;

        fs::write(&path, content).context("Failed to write profile registry")?;

        Ok(())
    }

    /// Validate that a profile can be accessed (exists and is not locked)
    fn validate_profile_access(&self, profile: &Option<String>) -> Result<(), String> {
        if let Some(name) = profile {
            // Check if profile exists
            if !self.profile_registry.contains_key(name) {
                return Err(format!(
                    "Unknown profile '{}'. Create it first with: webprobe profile create {}",
                    name, name
                ));
            }

            // Check if profile is locked
            if let Some(metadata) = self.profile_registry.get(name)
                && let Some(locked_until) = metadata.locked_until
            {
                let now = chrono::Utc::now();
                if locked_until > now {
                    return Err(format!(
                        "Profile '{}' is locked until {}. Use --force or wait for lock to expire.",
                        name,
                        locked_until.format("%Y-%m-%d %H:%M:%S UTC")
                    ));
                }
            }
        }
        Ok(())
    }

    /// Clean up profiles that haven't been accessed in the specified duration
    async fn cleanup_unused_profiles(&mut self, ttl_hours: i64) {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::hours(ttl_hours);

        let mut profiles_to_remove = Vec::new();

        // Find profiles that haven't been accessed recently
        for (name, metadata) in &self.profile_registry {
            // Skip locked profiles
            if let Some(locked_until) = metadata.locked_until
                && locked_until > now
            {
                continue;
            }

            // Check last accessed time
            if let Some(last_accessed) = metadata.last_accessed {
                if last_accessed < cutoff {
                    profiles_to_remove.push(name.clone());
                }
            } else if metadata.created_at < cutoff {
                // If never accessed, check creation time
                profiles_to_remove.push(name.clone());
            }
        }

        // Remove expired profiles
        let had_profiles_to_remove = !profiles_to_remove.is_empty();
        for name in profiles_to_remove {
            info!("Cleaning up unused profile: {}", name);

            // Shutdown the browser manager if it exists
            if let Some(profile) = self.profiles.remove(&name)
                && let Err(e) = profile.browser.shutdown().await
            {
                error!("Error shutting down browser for profile '{}': {}", name, e);
            }

            // Remove from registry
            self.profile_registry.remove(&name);
        }

        // Save updated registry if any profiles were removed
        if had_profiles_to_remove && let Err(e) = self.save_profile_registry() {
            warn!("Failed to save profile registry after cleanup: {}", e);
        }
    }

    /// Get or create a profile by name
    async fn get_or_create_profile(&mut self, name: String) -> Result<&mut ProfileState> {
        // Check if profile already exists
        if self.profiles.contains_key(&name) {
            // Update last accessed time
            if let Some(profile) = self.profiles.get_mut(&name) {
                profile.last_accessed = chrono::Utc::now();
            }

            // Update registry if it's a registered profile
            if let Some(metadata) = self.profile_registry.get_mut(&name) {
                metadata.last_accessed = Some(chrono::Utc::now());
                let _ = self.save_profile_registry();
            }

            return Ok(self.profiles.get_mut(&name).unwrap());
        }

        // Check if this is a registered profile
        if let Some(metadata) = self.profile_registry.get(&name) {
            // Create profile from registry
            let config = ProfileConfig {
                browser_type: metadata.browser_type,
                viewport: metadata.viewport,
                headless: true,
                ..Default::default()
            };

            let profile = ProfileState::new(&config, Some(name.clone()))
                .await
                .context(format!("Failed to create profile '{}'", name))?;

            self.profiles.insert(name.clone(), profile);
            Ok(self.profiles.get_mut(&name).unwrap())
        } else {
            Err(anyhow::anyhow!(
                "Unknown profile '{}'. Create it first with: webprobe profile create {}",
                name,
                name
            ))
        }
    }

    /// Create a temporary profile that will be cleaned up after use
    async fn create_temp_profile(&mut self) -> Result<String> {
        self.temp_profile_counter += 1;
        let name = format!("temp-{}", self.temp_profile_counter);

        let profile = ProfileState::new_temporary(self.browser_type)
            .await
            .context("Failed to create temporary profile")?;

        self.profiles.insert(name.clone(), profile);
        Ok(name)
    }

    /// Clean up a temporary profile
    async fn cleanup_temp_profile(&mut self, name: &str) {
        if let Some(profile) = self.profiles.remove(name)
            && profile.is_temporary
            && let Err(e) = profile.browser.shutdown().await
        {
            warn!("Failed to shutdown temporary profile '{}': {}", name, e);
        }
    }

    /// Shutdown all browser managers
    pub async fn shutdown(self) -> Result<()> {
        // Shutdown all profile browsers
        for (name, profile) in self.profiles {
            if let Err(e) = profile.browser.shutdown().await {
                error!("Error shutting down profile '{}': {}", name, e);
            }
        }

        Ok(())
    }

    /// Handle a request using the browser managers
    async fn handle_request(&mut self, request: DaemonRequest) -> DaemonResponse {
        // Perform periodic cleanup (every hour)
        let now = chrono::Utc::now();
        if now - self.last_cleanup > chrono::Duration::hours(1) {
            // Clean up profiles not accessed in last 24 hours
            self.cleanup_unused_profiles(24).await;
            self.last_cleanup = now;
        }

        match request {
            DaemonRequest::Ping => DaemonResponse::Pong,

            DaemonRequest::Shutdown => {
                info!("Shutdown requested");
                // Clean up will happen when daemon is dropped
                DaemonResponse::Success("Daemon shutting down".to_string())
            }

            DaemonRequest::ListTabs { profile } => {
                // Get the appropriate browser manager
                let browser = match self.get_browser(profile.clone()).await {
                    Ok(b) => b,
                    Err(e) => {
                        return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                    }
                };

                // Get tab list from the browser
                let tabs = browser.list_tabs().await;
                let profile_name = Profile::from_optional_string(profile).name();

                // Convert to TabInfo format
                let tab_infos: Vec<TabInfo> = tabs
                    .into_iter()
                    .map(|name| {
                        TabInfo {
                            name,
                            url: None, // TODO: Track URLs in BrowserManager
                            profile: Some(profile_name.clone()),
                            browser_type: self.browser_type.to_string(),
                            viewport: None, // TODO: Track per-tab viewport
                        }
                    })
                    .collect();

                DaemonResponse::TabList(tab_infos)
            }

            DaemonRequest::CreateTab { name, profile, .. } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Get the appropriate browser manager
                let browser = match self.get_browser(profile).await {
                    Ok(b) => b,
                    Err(e) => {
                        return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                    }
                };

                // Create the tab
                match browser.create_tab(name.clone()).await {
                    Ok(_) => DaemonResponse::Success(format!("Tab '{}' created", name)),
                    Err(e) => DaemonResponse::Error(format!("Failed to create tab: {}", e)),
                }
            }

            DaemonRequest::CloseTab { name } => {
                // We need to find which browser has this tab
                // First check default browser
                if self.default_browser().has_tab(&name).await {
                    match self.default_browser().close_tab(&name).await {
                        Ok(_) => return DaemonResponse::Success(format!("Tab '{}' closed", name)),
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to close tab: {}", e));
                        }
                    }
                }

                // Check oneshot browser (though tabs there should be temporary)
                if self.oneshot_browser().has_tab(&name).await {
                    match self.oneshot_browser().close_tab(&name).await {
                        Ok(_) => return DaemonResponse::Success(format!("Tab '{}' closed", name)),
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to close tab: {}", e));
                        }
                    }
                }

                // Check custom profiles
                for (profile_name, profile) in self.profiles.iter_mut() {
                    if profile_name != "default" && profile_name != "oneshot" {
                        let browser = &mut profile.browser;
                        if browser.has_tab(&name).await {
                            match browser.close_tab(&name).await {
                                Ok(_) => {
                                    return DaemonResponse::Success(format!(
                                        "Tab '{}' closed",
                                        name
                                    ));
                                }
                                Err(e) => {
                                    return DaemonResponse::Error(format!(
                                        "Failed to close tab: {}",
                                        e
                                    ));
                                }
                            }
                        }
                    }
                }

                DaemonResponse::Error(format!("Tab '{}' not found", name))
            }

            DaemonRequest::Click {
                tab_name,
                url,
                selector,
                index,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation (no tab name provided)
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Capture variables needed in the closures
                let url_clone = url.clone();
                let selector_clone = selector.clone();

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Use with_temp_tab for one-shot operations
                    browser
                        .with_temp_tab(move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL is provided
                                if let Some(url) = url_clone {
                                    ctx.goto(&url).await?;
                                }
                                // Perform the click
                                ctx.click_element(&selector_clone, index).await
                            })
                        })
                        .await
                } else {
                    // Create tab if needed
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Capture for the closure
                    let url_clone2 = url.clone();
                    let selector_clone2 = selector.clone();

                    // Use with_tab for persistent operations
                    browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL is provided
                                if let Some(url) = url_clone2 {
                                    ctx.goto(&url).await?;
                                }
                                // Perform the click
                                ctx.click_element(&selector_clone2, index).await
                            })
                        })
                        .await
                };

                match result {
                    Ok(_) => DaemonResponse::Success(format!("Clicked element: {}", selector)),
                    Err(e) => DaemonResponse::Error(format!("Failed to click: {}", e)),
                }
            }

            DaemonRequest::Type {
                tab_name,
                url,
                selector,
                text,
                clear,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation (no tab name provided)
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Capture variables for the async block
                let selector_clone = selector.clone();
                let url_clone = url.clone();
                let text_clone = text.clone();

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Use with_temp_tab for one-shot operations
                    browser
                        .with_temp_tab(move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL is provided
                                if let Some(url) = url_clone {
                                    ctx.goto(&url).await?;
                                }
                                // Type the text
                                ctx.type_text(&selector_clone, &text_clone, clear).await
                            })
                        })
                        .await
                } else {
                    // Create tab if needed
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Capture for the closure
                    let url_clone2 = url.clone();
                    let selector_clone2 = selector.clone();
                    let text_clone2 = text.clone();

                    // Use with_tab for persistent operations
                    browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL is provided
                                if let Some(url) = url_clone2 {
                                    ctx.goto(&url).await?;
                                }
                                // Type the text
                                ctx.type_text(&selector_clone2, &text_clone2, clear).await
                            })
                        })
                        .await
                };

                match result {
                    Ok(_) => DaemonResponse::Success(format!("Typed text into: {}", selector)),
                    Err(e) => DaemonResponse::Error(format!("Failed to type: {}", e)),
                }
            }

            DaemonRequest::Inspect {
                tab_name,
                url,
                selector,
                all,
                index,
                expect_one,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation (no tab name provided)
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Capture variables for the async block
                let url_clone = url.clone();
                let selector_clone = selector.clone();

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Use with_temp_tab for one-shot operations
                    browser
                        .with_temp_tab(move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL is provided and not empty
                                if !url_clone.is_empty() {
                                    ctx.goto(&url_clone).await?;
                                }
                                // Perform the inspection
                                ctx.inspect_element(
                                    &selector_clone,
                                    InspectionDepth::Shallow,
                                    all,
                                    index,
                                    expect_one,
                                )
                                .await
                            })
                        })
                        .await
                } else {
                    // Create tab if needed
                    info!("Getting or creating tab: {}", tab_name);
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        error!("Failed to get/create tab '{}': {}", tab_name, e);
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }
                    info!("Tab '{}' ready, calling with_tab", tab_name);

                    // Capture for the closure
                    let url_clone2 = url.clone();
                    let selector_clone2 = selector.clone();

                    // Use with_tab for persistent operations
                    browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL is provided and not empty
                                if !url_clone2.is_empty() {
                                    ctx.goto(&url_clone2).await?;
                                }
                                // Perform the inspection
                                ctx.inspect_element(
                                    &selector_clone2,
                                    InspectionDepth::Shallow,
                                    all,
                                    index,
                                    expect_one,
                                )
                                .await
                            })
                        })
                        .await
                };

                match result {
                    Ok(elements) => DaemonResponse::InspectResult(elements, None),
                    Err(e) => DaemonResponse::Error(format!("Failed to inspect: {}", e)),
                }
            }

            DaemonRequest::Scroll {
                tab_name,
                url,
                selector,
                by_x,
                by_y,
                to,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }
                // Use unified tab name preparation
                let (actual_tab_name, is_oneshot) = Self::prepare_tab_name(&tab_name);

                // Get the appropriate browser manager
                let browser = match self.get_browser_for_operation(is_oneshot, profile).await {
                    Ok(b) => b,
                    Err(e) => {
                        return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                    }
                };

                // Get or create the tab
                if let Err(e) = browser.get_or_create_tab(&actual_tab_name).await {
                    return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                }

                // Mark as temporary if needed
                if is_oneshot {
                    browser.mark_tab_temporary(&actual_tab_name).await;
                }

                // Navigate if URL is provided
                if !url.is_empty()
                    && let Err(e) = browser.goto(&url).await
                {
                    return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                }

                // Perform scroll
                match browser
                    .browser()
                    .scroll("", selector.as_deref(), by_x, by_y, to.as_deref())
                    .await
                {
                    Ok(_) => {
                        if is_oneshot
                            && let Err(e) = browser.cleanup_if_temporary(&actual_tab_name).await
                        {
                            warn!("Failed to cleanup temporary tab: {}", e);
                        }
                        DaemonResponse::Success("Scrolled successfully".to_string())
                    }
                    Err(e) => DaemonResponse::Error(format!("Failed to scroll: {}", e)),
                }
            }

            DaemonRequest::Eval {
                tab_name,
                url,
                code,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();
                let actual_tab_name = if is_oneshot {
                    format!("oneshot-{}", uuid::Uuid::new_v4())
                } else {
                    tab_name.clone()
                };

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Get or create the tab
                if let Err(e) = browser.get_or_create_tab(&actual_tab_name).await {
                    return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                }

                // Mark as temporary if needed
                if is_oneshot {
                    browser.mark_tab_temporary(&actual_tab_name).await;
                }

                // Navigate if URL is provided
                if let Some(url) = url
                    && !url.is_empty()
                    && let Err(e) = browser.goto(&url).await
                {
                    return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                }

                // Execute JavaScript
                match browser.browser().execute_javascript(Some(""), &code).await {
                    Ok(result) => {
                        if is_oneshot
                            && let Err(e) = browser.cleanup_if_temporary(&actual_tab_name).await
                        {
                            warn!("Failed to cleanup temporary tab: {}", e);
                        }
                        DaemonResponse::EvalResult(result)
                    }
                    Err(e) => DaemonResponse::Error(format!("Failed to execute script: {}", e)),
                }
            }

            DaemonRequest::Analyze {
                tab_name,
                url,
                selector,
                focus,
                proximity,
                index,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();
                let actual_tab_name = if is_oneshot {
                    format!("oneshot-{}", uuid::Uuid::new_v4())
                } else {
                    tab_name.clone()
                };

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Get or create the tab
                if let Err(e) = browser.get_or_create_tab(&actual_tab_name).await {
                    return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                }

                // Mark as temporary if needed
                if is_oneshot {
                    browser.mark_tab_temporary(&actual_tab_name).await;
                }

                // Navigate if URL is provided
                if !url.is_empty()
                    && let Err(e) = browser.goto(&url).await
                {
                    return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                }

                // Perform analysis
                match browser
                    .browser()
                    .analyze_context("", &selector, &focus, proximity.unwrap_or(100), index)
                    .await
                {
                    Ok(result) => {
                        if is_oneshot
                            && let Err(e) = browser.cleanup_if_temporary(&actual_tab_name).await
                        {
                            warn!("Failed to cleanup temporary tab: {}", e);
                        }
                        DaemonResponse::AnalyzeResult(result)
                    }
                    Err(e) => DaemonResponse::Error(format!("Failed to analyze: {}", e)),
                }
            }

            DaemonRequest::Layout {
                tab_name,
                url,
                selector,
                depth,
                max_elements,
                wait_stable,
                detect_shadow,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();
                let actual_tab_name = if is_oneshot {
                    format!("oneshot-{}", uuid::Uuid::new_v4())
                } else {
                    tab_name.clone()
                };

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Get or create the tab
                if let Err(e) = browser.get_or_create_tab(&actual_tab_name).await {
                    return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                }

                // Mark as temporary if needed
                if is_oneshot {
                    browser.mark_tab_temporary(&actual_tab_name).await;
                }

                // Navigate if URL is provided
                if !url.is_empty()
                    && let Err(e) = browser.goto(&url).await
                {
                    return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                }

                // Perform layout analysis
                match browser
                    .browser()
                    .analyze_layout(
                        "",
                        &selector,
                        depth,
                        max_elements,
                        wait_stable,
                        detect_shadow,
                    )
                    .await
                {
                    Ok(result) => {
                        if is_oneshot
                            && let Err(e) = browser.cleanup_if_temporary(&actual_tab_name).await
                        {
                            warn!("Failed to cleanup temporary tab: {}", e);
                        }
                        DaemonResponse::LayoutResult(result)
                    }
                    Err(e) => DaemonResponse::Error(format!("Failed to analyze layout: {}", e)),
                }
            }

            DaemonRequest::Status { tab_name, profile } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // If profile is specified, check that browser first
                let browser = if let Some(profile_name) = profile {
                    match self.get_browser(Some(profile_name)).await {
                        Ok(b) if b.has_tab(&tab_name).await => Some(b),
                        _ => None,
                    }
                } else {
                    // Find which browser has this tab
                    if self.default_browser().has_tab(&tab_name).await {
                        Some(self.default_browser())
                    } else if self.oneshot_browser().has_tab(&tab_name).await {
                        Some(self.oneshot_browser())
                    } else {
                        // Check custom profiles
                        for (profile_name, profile) in self.profiles.iter_mut() {
                            if profile_name != "default" && profile_name != "oneshot" {
                                let browser = &mut profile.browser;
                                if browser.has_tab(&tab_name).await {
                                    let status = serde_json::json!({
                                        "tab": tab_name,
                                        "status": "active",
                                        "profile": "custom"
                                    });
                                    return DaemonResponse::StatusResult(status);
                                }
                            }
                        }
                        None
                    }
                };

                if let Some(browser) = browser {
                    // Use with_tab to safely access the tab
                    let result =
                        browser
                            .with_tab(&tab_name, move |ctx| {
                                Box::pin(async move {
                                    // Get current URL and title
                                    let url = ctx
                                        .execute_script("window.location.href")
                                        .await
                                        .ok()
                                        .and_then(|v| v.as_str().map(String::from))
                                        .unwrap_or_default();
                                    let title = ctx
                                        .execute_script("document.title")
                                        .await
                                        .ok()
                                        .and_then(|v| v.as_str().map(String::from))
                                        .unwrap_or_default();

                                    // Get localStorage keys
                                    let local_storage = ctx
                                        .execute_script("Object.keys(localStorage || {})")
                                        .await
                                        .ok();

                                    // Get sessionStorage keys
                                    let session_storage = ctx
                                        .execute_script("Object.keys(sessionStorage || {})")
                                        .await
                                        .ok();

                                    // Get cookie count
                                    let cookies = ctx.execute_script(
                                "document.cookie.split(';').filter(c => c.trim()).length"
                            ).await.ok();

                                    Ok(serde_json::json!({
                                        "url": url,
                                        "title": title,
                                        "localStorage": local_storage,
                                        "sessionStorage": session_storage,
                                        "cookies": cookies
                                    }))
                                })
                            })
                            .await;

                    match result {
                        Ok(status) => DaemonResponse::StatusResult(status),
                        Err(e) => DaemonResponse::Error(format!("Failed to get status: {}", e)),
                    }
                } else {
                    DaemonResponse::Error(format!("Tab '{}' not found", tab_name))
                }
            }

            DaemonRequest::Detect {
                url,
                context,
                tab_name,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if we should use an existing tab or create a temp one
                let use_existing_tab = !tab_name.is_empty();

                if use_existing_tab {
                    // Get the appropriate browser manager for the profile
                    let browser = match self.get_browser(profile).await {
                        Ok(b) if b.has_tab(&tab_name).await => Some(b),
                        _ => {
                            // If not found in the specified profile, check other browsers
                            if self.default_browser().has_tab(&tab_name).await {
                                Some(self.default_browser())
                            } else if self.oneshot_browser().has_tab(&tab_name).await {
                                Some(self.oneshot_browser())
                            } else {
                                None
                            }
                        }
                    };

                    if let Some(browser) = browser {
                        let url_clone = url.clone();
                        let context_clone = context.clone();

                        let result = browser
                            .with_tab(&tab_name, move |ctx| {
                                Box::pin(async move {
                                    // Navigate if URL provided and not empty
                                    if !url_clone.is_empty() {
                                        ctx.goto(&url_clone).await?;
                                    }

                                    // Detect smart elements
                                    ctx.detect_smart_elements(context_clone.as_deref()).await
                                })
                            })
                            .await;

                        match result {
                            Ok(elements) => DaemonResponse::DetectResult(elements),
                            Err(e) => DaemonResponse::Error(format!("Failed to detect: {}", e)),
                        }
                    } else {
                        DaemonResponse::Error(format!("Tab '{}' not found", tab_name))
                    }
                } else {
                    // One-shot operation
                    let url_clone = url.clone();
                    let context_clone = context.clone();

                    let result = self
                        .oneshot_browser()
                        .with_temp_tab(move |ctx| {
                            Box::pin(async move {
                                // Navigate to URL
                                ctx.goto(&url_clone).await?;
                                // Detect smart elements
                                ctx.detect_smart_elements(context_clone.as_deref()).await
                            })
                        })
                        .await;

                    match result {
                        Ok(elements) => DaemonResponse::DetectResult(elements),
                        Err(e) => DaemonResponse::Error(format!("Failed to detect: {}", e)),
                    }
                }
            }

            DaemonRequest::Html {
                tab_name,
                url,
                selector,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Create a temp tab name for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    browser.get_or_create_tab(&temp_tab).await.ok();
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate and get HTML
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    let html_result = browser
                        .browser()
                        .get_page_html("", selector.as_deref())
                        .await;
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }
                    html_result
                } else {
                    // Get or create the tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL is provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Get HTML
                    browser
                        .browser()
                        .get_page_html("", selector.as_deref())
                        .await
                };

                match result {
                    Ok(html) => DaemonResponse::HtmlResult(html),
                    Err(e) => DaemonResponse::Error(format!("Failed to get HTML: {}", e)),
                }
            }

            DaemonRequest::Wait {
                tab_name,
                url,
                selector,
                timeout,
                condition,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Create a temp tab name for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    browser.get_or_create_tab(&temp_tab).await.ok();
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate and wait
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    let wait_result = browser
                        .browser()
                        .wait_for_element("", &selector, timeout, &condition)
                        .await;
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }
                    wait_result
                } else {
                    // Get or create the tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL is provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Wait for element
                    browser
                        .browser()
                        .wait_for_element("", &selector, timeout, &condition)
                        .await
                };

                match result {
                    Ok(found) => DaemonResponse::WaitResult(found),
                    Err(e) => DaemonResponse::Error(format!("Failed to wait for element: {}", e)),
                }
            }

            DaemonRequest::FindText {
                tab_name,
                url,
                text,
                fuzzy,
                case_sensitive,
                element_type,
                all: _,
                index: _,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Create a temp tab name for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    browser.get_or_create_tab(&temp_tab).await.ok();
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate and find text
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    let find_result = browser
                        .browser()
                        .find_by_text("", &text, element_type.as_deref(), fuzzy, case_sensitive)
                        .await;
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }
                    find_result
                } else {
                    // Get or create the tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL is provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Find text
                    browser
                        .browser()
                        .find_by_text("", &text, element_type.as_deref(), fuzzy, case_sensitive)
                        .await
                };

                match result {
                    Ok(results) => DaemonResponse::FindTextResult(results),
                    Err(e) => DaemonResponse::Error(format!("Failed to find text: {}", e)),
                }
            }

            DaemonRequest::WaitIdle {
                tab_name,
                url,
                timeout,
                idle_time,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Create a temp tab name for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    browser.get_or_create_tab(&temp_tab).await.ok();
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate and wait for idle
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    let wait_result = browser
                        .browser()
                        .wait_for_network_idle(timeout, idle_time)
                        .await;
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }
                    wait_result
                } else {
                    // Get or create the tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL is provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Wait for network idle
                    browser
                        .browser()
                        .wait_for_network_idle(timeout, idle_time)
                        .await
                };

                match result {
                    Ok(idle) => {
                        DaemonResponse::WaitIdleResult(vec![format!("Network idle: {}", idle)])
                    }
                    Err(e) => {
                        DaemonResponse::Error(format!("Failed to wait for network idle: {}", e))
                    }
                }
            }

            DaemonRequest::WaitNavigation {
                tab_name,
                url,
                to,
                timeout,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use the right method based on whether it's one-shot
                let result = if is_oneshot {
                    // Create a temp tab name for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    browser.get_or_create_tab(&temp_tab).await.ok();
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate and wait
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    let wait_result = browser
                        .browser()
                        .wait_for_navigation(None, to.as_deref(), timeout)
                        .await;
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }
                    wait_result
                } else {
                    // Get or create the tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL is provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Wait for navigation
                    browser
                        .browser()
                        .wait_for_navigation(None, to.as_deref(), timeout)
                        .await
                };

                match result {
                    Ok(new_url) => DaemonResponse::WaitNavigationResult(new_url),
                    Err(e) => {
                        DaemonResponse::Error(format!("Failed to wait for navigation: {}", e))
                    }
                }
            }

            DaemonRequest::Batch {
                tab_name,
                commands,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Parse the commands JSON - expecting an array of command objects
                let batch_commands: Vec<serde_json::Value> = match serde_json::from_str(&commands) {
                    Ok(cmds) => cmds,
                    Err(e) => {
                        return DaemonResponse::Error(format!(
                            "Failed to parse batch commands: {}",
                            e
                        ));
                    }
                };

                // Get the appropriate browser manager
                let browser = match self.get_browser(profile).await {
                    Ok(b) => b,
                    Err(e) => {
                        return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                    }
                };

                // Get or create the tab
                if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                    return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                }

                // Execute batch commands one by one
                let mut results = Vec::new();
                for cmd in batch_commands {
                    let cmd_type = cmd.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let result = match cmd_type {
                        "goto" | "navigate" => {
                            if let Some(url) = cmd.get("url").and_then(|v| v.as_str()) {
                                match browser.goto(url).await {
                                    Ok(_) => {
                                        json!({"success": true, "message": format!("Navigated to {}", url)})
                                    }
                                    Err(e) => json!({"error": e.to_string()}),
                                }
                            } else {
                                json!({"error": "goto requires 'url' field"})
                            }
                        }
                        "type" => {
                            let selector = cmd.get("selector").and_then(|v| v.as_str());
                            let text = cmd.get("text").and_then(|v| v.as_str());
                            let clear = cmd.get("clear").and_then(|v| v.as_bool()).unwrap_or(false);

                            if let (Some(selector), Some(text)) = (selector, text) {
                                match browser.browser().type_text("", selector, text, clear).await {
                                    Ok(_) => {
                                        json!({"success": true, "message": format!("Typed text into {}", selector)})
                                    }
                                    Err(e) => json!({"error": e.to_string()}),
                                }
                            } else {
                                json!({"error": "type requires 'selector' and 'text' fields"})
                            }
                        }
                        "click" => {
                            if let Some(selector) = cmd.get("selector").and_then(|v| v.as_str()) {
                                let index = cmd
                                    .get("index")
                                    .and_then(|v| v.as_u64())
                                    .map(|i| i as usize);
                                match browser.browser().click_element("", selector, index).await {
                                    Ok(_) => {
                                        json!({"success": true, "message": format!("Clicked {}", selector)})
                                    }
                                    Err(e) => json!({"error": e.to_string()}),
                                }
                            } else {
                                json!({"error": "click requires 'selector' field"})
                            }
                        }
                        "wait" => {
                            if let Some(selector) = cmd.get("selector").and_then(|v| v.as_str()) {
                                let timeout =
                                    cmd.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30000);
                                let condition = cmd
                                    .get("condition")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("present");
                                match browser
                                    .browser()
                                    .wait_for_element("", selector, timeout, condition)
                                    .await
                                {
                                    Ok(_) => {
                                        json!({"success": true, "message": format!("Found element {}", selector)})
                                    }
                                    Err(e) => json!({"error": e.to_string()}),
                                }
                            } else {
                                json!({"error": "wait requires 'selector' field"})
                            }
                        }
                        "sleep" => {
                            if let Some(ms) = cmd.get("milliseconds").and_then(|v| v.as_u64()) {
                                tokio::time::sleep(tokio::time::Duration::from_millis(ms)).await;
                                json!({"success": true, "message": format!("Slept for {} ms", ms)})
                            } else {
                                json!({"error": "sleep requires 'milliseconds' field"})
                            }
                        }
                        _ => {
                            json!({"error": format!("Unknown command type: {}", cmd_type)})
                        }
                    };
                    results.push(result);
                }

                DaemonResponse::BatchResult(results)
            }

            DaemonRequest::Screenshot {
                tab_name,
                url,
                selector,
                output,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use appropriate tab approach
                if is_oneshot {
                    // Create temporary tab for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    if let Err(e) = browser.get_or_create_tab(&temp_tab).await {
                        return DaemonResponse::Error(format!("Failed to create temp tab: {}", e));
                    }
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate if URL provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                            warn!("Failed to cleanup temporary tab: {}", e);
                        }
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Take screenshot
                    let result = if let Some(sel) = &selector {
                        browser
                            .browser()
                            .screenshot_element(sel, Some(&output))
                            .await
                    } else {
                        browser.browser().screenshot(Some(&output)).await
                    };

                    // Cleanup temp tab
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }

                    match result {
                        Ok(data) => DaemonResponse::ScreenshotResult {
                            saved_to: output,
                            bytes: data.len(),
                        },
                        Err(e) => {
                            DaemonResponse::Error(format!("Failed to take screenshot: {}", e))
                        }
                    }
                } else {
                    // Use persistent tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Take screenshot using with_tab for proper locking
                    let selector_clone = selector.clone();
                    let _result: Result<Vec<u8>> = browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(async move {
                                if let Some(sel) = selector_clone {
                                    // We need to use the browser directly for screenshot
                                    // This is a limitation - we can't access browser from TabContext
                                    // For now, use execute_script to check element exists
                                    let check = format!(
                                        r#"
                                    document.querySelector('{}') !== null
                                "#,
                                        sel
                                    );
                                    let exists = ctx.execute_script(&check).await?;
                                    if !exists.as_bool().unwrap_or(false) {
                                        return Err(anyhow::anyhow!("Element not found: {}", sel));
                                    }
                                    // Return a placeholder - actual screenshot happens outside with_tab
                                    Ok(vec![])
                                } else {
                                    Ok(vec![])
                                }
                            })
                        })
                        .await;

                    // Now take the actual screenshot (not ideal but works)
                    let actual_result = if let Some(sel) = &selector {
                        browser
                            .browser()
                            .screenshot_element(sel, Some(&output))
                            .await
                    } else {
                        browser.browser().screenshot(Some(&output)).await
                    };

                    match actual_result {
                        Ok(data) => DaemonResponse::ScreenshotResult {
                            saved_to: output,
                            bytes: data.len(),
                        },
                        Err(e) => {
                            DaemonResponse::Error(format!("Failed to take screenshot: {}", e))
                        }
                    }
                }
            }

            DaemonRequest::Iframe {
                tab_name,
                url,
                iframe_selector,
                element_selector,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use appropriate approach
                if is_oneshot {
                    // Create temporary tab for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    if let Err(e) = browser.get_or_create_tab(&temp_tab).await {
                        return DaemonResponse::Error(format!("Failed to create temp tab: {}", e));
                    }
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Navigate if URL provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                            warn!("Failed to cleanup temporary tab: {}", e);
                        }
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Inspect iframe
                    let result = browser
                        .browser()
                        .inspect_iframe(&iframe_selector, &element_selector)
                        .await;

                    // Cleanup temp tab
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }

                    match result {
                        Ok(elements) => DaemonResponse::IframeResult(elements),
                        Err(e) => DaemonResponse::Error(format!("Failed to inspect iframe: {}", e)),
                    }
                } else {
                    // Use persistent tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Inspect iframe with proper locking
                    let iframe_sel = iframe_selector.clone();
                    let element_sel = element_selector.clone();
                    let _result: Result<Vec<ElementInfo>> = browser.with_tab(&tab_name, move |ctx| {
                        Box::pin(async move {
                            // Use JavaScript through TabContext
                            let script = format!(r#"
                                const iframe = document.querySelector('{}');
                                if (!iframe) {{
                                    throw new Error('Iframe not found: {}');
                                }}
                                const iframeDoc = iframe.contentDocument || iframe.contentWindow.document;
                                if (!iframeDoc) {{
                                    throw new Error('Cannot access iframe content (may be cross-origin)');
                                }}
                                const elements = iframeDoc.querySelectorAll('{}');
                                return elements.length;
                            "#, iframe_sel, iframe_sel, element_sel);

                            let count = ctx.execute_script(&script).await?;
                            let count = count.as_i64().unwrap_or(0);

                            if count == 0 {
                                return Err(anyhow::anyhow!("No elements found in iframe"));
                            }
                            // Return placeholder - actual inspection happens outside with_tab
                            Ok(vec![])
                        })
                    }).await;

                    // Now do the actual iframe inspection
                    let actual_result = browser
                        .browser()
                        .inspect_iframe(&iframe_selector, &element_selector)
                        .await;

                    match actual_result {
                        Ok(elements) => DaemonResponse::IframeResult(elements),
                        Err(e) => DaemonResponse::Error(format!("Failed to inspect iframe: {}", e)),
                    }
                }
            }

            DaemonRequest::Diagnose {
                tab_name,
                url,
                selector,
                check_type,
                viewport,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use appropriate approach
                if is_oneshot {
                    // Create temporary tab for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    if let Err(e) = browser.get_or_create_tab(&temp_tab).await {
                        return DaemonResponse::Error(format!("Failed to create temp tab: {}", e));
                    }
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Use with_tab to ensure proper context
                    let vp = viewport.as_ref().and_then(|v| ViewportSize::parse(v).ok());
                    let sel = selector.clone();
                    let check = check_type.clone();
                    let navigate_url = url.clone();

                    let result = browser
                        .with_tab(&temp_tab, move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL provided
                                if !navigate_url.is_empty() {
                                    ctx.goto(&navigate_url).await?;
                                }

                                // Set viewport if provided
                                if let Some(viewport_size) = vp {
                                    let script = format!(
                                        "window.resizeTo({}, {});",
                                        viewport_size.width, viewport_size.height
                                    );
                                    ctx.execute_script(&script).await?;
                                }

                                // Run diagnose in the correct tab context
                                ctx.diagnose_layout(sel.as_deref(), &check).await
                            })
                        })
                        .await;

                    // Always cleanup temporary tab
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab '{}': {}", temp_tab, e);
                    }

                    match result {
                        Ok(diagnosis) => DaemonResponse::DiagnoseResult(diagnosis),
                        Err(e) => DaemonResponse::Error(format!("Failed to diagnose: {}", e)),
                    }
                } else {
                    // Use persistent tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Use with_tab for proper context
                    let vp = viewport.as_ref().and_then(|v| ViewportSize::parse(v).ok());
                    let sel = selector.clone();
                    let check = check_type.clone();
                    let navigate_url = url.clone();

                    match browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL provided
                                if !navigate_url.is_empty() {
                                    ctx.goto(&navigate_url).await?;
                                }

                                // Set viewport if provided
                                if let Some(viewport_size) = vp {
                                    let script = format!(
                                        "window.resizeTo({}, {});",
                                        viewport_size.width, viewport_size.height
                                    );
                                    ctx.execute_script(&script).await?;
                                }

                                // Run diagnose in the correct tab context
                                ctx.diagnose_layout(sel.as_deref(), &check).await
                            })
                        })
                        .await
                    {
                        Ok(diagnosis) => DaemonResponse::DiagnoseResult(diagnosis),
                        Err(e) => DaemonResponse::Error(format!("Failed to diagnose: {}", e)),
                    }
                }
            }

            // For now, return an error for other unimplemented requests
            DaemonRequest::Compare {
                tab_name,
                url1,
                url2,
                mode,
                selector,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use appropriate approach
                if is_oneshot {
                    // Create temporary tab for one-shot
                    let temp_tab = format!("oneshot-{}", uuid::Uuid::new_v4());
                    if let Err(e) = browser.get_or_create_tab(&temp_tab).await {
                        return DaemonResponse::Error(format!("Failed to create temp tab: {}", e));
                    }
                    browser.mark_tab_temporary(&temp_tab).await;

                    // Compare pages
                    let result = browser
                        .browser()
                        .compare_pages(&url1, &url2, &mode, selector.as_deref())
                        .await;

                    // Cleanup temp tab
                    if let Err(e) = browser.cleanup_if_temporary(&temp_tab).await {
                        warn!("Failed to cleanup temporary tab: {}", e);
                    }

                    match result {
                        Ok(comparison) => DaemonResponse::CompareResult(comparison),
                        Err(e) => DaemonResponse::Error(format!("Failed to compare: {}", e)),
                    }
                } else {
                    // Use persistent tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Compare with proper locking
                    let u1 = url1.clone();
                    let u2 = url2.clone();
                    let m = mode.clone();
                    let s = selector.clone();
                    let result = browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(
                                async move { ctx.compare_pages(&u1, &u2, &m, s.as_deref()).await },
                            )
                        })
                        .await;

                    match result {
                        Ok(comparison) => DaemonResponse::CompareResult(comparison),
                        Err(e) => DaemonResponse::Error(format!("Failed to compare: {}", e)),
                    }
                }
            }

            DaemonRequest::Validate {
                tab_name,
                url,
                check_type,
                profile,
            } => {
                // Validate profile access if specified
                if let Err(e) = self.validate_profile_access(&profile) {
                    return DaemonResponse::Error(e);
                }

                // Determine if this is a one-shot operation
                let is_oneshot = tab_name.is_empty();

                // Get the appropriate browser manager
                let browser = if is_oneshot {
                    self.oneshot_browser()
                } else {
                    match self.get_browser(profile).await {
                        Ok(b) => b,
                        Err(e) => {
                            return DaemonResponse::Error(format!("Failed to get browser: {}", e));
                        }
                    }
                };

                // Use appropriate approach
                if is_oneshot {
                    // Use with_temp_tab for proper navigation and validation
                    let url_clone = url.clone();
                    let check_clone = check_type.clone();

                    let result = browser
                        .with_temp_tab(move |ctx| {
                            Box::pin(async move {
                                // Navigate if URL provided
                                if !url_clone.is_empty() {
                                    ctx.goto(&url_clone).await?;
                                }
                                // Validate page
                                ctx.validate_page(&check_clone).await
                            })
                        })
                        .await;

                    match result {
                        Ok(validation) => DaemonResponse::ValidateResult(validation),
                        Err(e) => DaemonResponse::Error(format!("Failed to validate: {}", e)),
                    }
                } else {
                    // Use persistent tab
                    if let Err(e) = browser.get_or_create_tab(&tab_name).await {
                        return DaemonResponse::Error(format!("Failed to get/create tab: {}", e));
                    }

                    // Navigate if URL provided
                    if !url.is_empty()
                        && let Err(e) = browser.goto(&url).await
                    {
                        return DaemonResponse::Error(format!("Failed to navigate: {}", e));
                    }

                    // Validate with proper locking
                    let check = check_type.clone();
                    let result = browser
                        .with_tab(&tab_name, move |ctx| {
                            Box::pin(async move { ctx.validate_page(&check).await })
                        })
                        .await;

                    match result {
                        Ok(validation) => DaemonResponse::ValidateResult(validation),
                        Err(e) => DaemonResponse::Error(format!("Failed to validate: {}", e)),
                    }
                }
            }

            DaemonRequest::CreateProfile { name, config } => {
                // Check if profile already exists
                if self.profile_registry.contains_key(&name) {
                    return DaemonResponse::Error(format!("Profile '{}' already exists", name));
                }

                // Create profile metadata
                let metadata = ProfileMetadata {
                    name: name.clone(),
                    created_at: chrono::Utc::now(),
                    created_by: std::env::var("USER").unwrap_or_else(|_| "unknown".to_string()),
                    browser_type: config.browser_type,
                    locked_until: None,
                    viewport: config.viewport,
                    last_accessed: None,
                    tabs_count: 0,
                };

                // Add to registry
                self.profile_registry.insert(name.clone(), metadata);

                // Save registry to disk
                if let Err(e) = self.save_profile_registry() {
                    warn!("Failed to save profile registry: {}", e);
                }

                // Create the profile state for this profile
                match ProfileState::new(&config, Some(name.clone())).await {
                    Ok(profile_state) => {
                        self.profiles.insert(name.clone(), profile_state);
                        DaemonResponse::Success(format!("Profile '{}' created successfully", name))
                    }
                    Err(e) => {
                        // Remove from registry if profile creation failed
                        self.profile_registry.remove(&name);
                        DaemonResponse::Error(format!("Failed to create profile: {}", e))
                    }
                }
            }

            DaemonRequest::DestroyProfile { name, force } => {
                // Check if profile exists
                if !self.profile_registry.contains_key(&name) {
                    return DaemonResponse::Error(format!("Profile '{}' does not exist", name));
                }

                // Check if profile is locked (unless force is specified)
                if !force && let Err(e) = self.validate_profile_access(&Some(name.clone())) {
                    return DaemonResponse::Error(e);
                }

                // Shutdown the profile if it exists
                if let Some(profile) = self.profiles.remove(&name)
                    && let Err(e) = profile.browser.shutdown().await
                {
                    error!("Error shutting down browser for profile '{}': {}", name, e);
                }

                // Remove from registry
                self.profile_registry.remove(&name);

                // Save registry to disk
                if let Err(e) = self.save_profile_registry() {
                    warn!("Failed to save profile registry: {}", e);
                }

                DaemonResponse::Success(format!("Profile '{}' destroyed", name))
            }

            DaemonRequest::ListProfiles => {
                let profiles: Vec<ProfileMetadata> =
                    self.profile_registry.values().cloned().collect();
                DaemonResponse::ProfileList(profiles)
            }

            DaemonRequest::ProfileInfo { name } => match self.profile_registry.get(&name) {
                Some(metadata) => DaemonResponse::ProfileMetadata(metadata.clone()),
                None => DaemonResponse::Error(format!("Profile '{}' not found", name)),
            },

            DaemonRequest::LockProfile {
                name,
                duration_minutes,
            } => {
                match self.profile_registry.get_mut(&name) {
                    Some(metadata) => {
                        let locked_until =
                            chrono::Utc::now() + chrono::Duration::minutes(duration_minutes as i64);
                        metadata.locked_until = Some(locked_until);

                        // Save registry to disk
                        if let Err(e) = self.save_profile_registry() {
                            warn!("Failed to save profile registry: {}", e);
                        }

                        DaemonResponse::Success(format!(
                            "Profile '{}' locked until {}",
                            name,
                            locked_until.format("%Y-%m-%d %H:%M:%S UTC")
                        ))
                    }
                    None => DaemonResponse::Error(format!("Profile '{}' not found", name)),
                }
            }

            DaemonRequest::UnlockProfile { name } => {
                match self.profile_registry.get_mut(&name) {
                    Some(metadata) => {
                        metadata.locked_until = None;

                        // Save registry to disk
                        if let Err(e) = self.save_profile_registry() {
                            warn!("Failed to save profile registry: {}", e);
                        }

                        DaemonResponse::Success(format!("Profile '{}' unlocked", name))
                    }
                    None => DaemonResponse::Error(format!("Profile '{}' not found", name)),
                }
            }

            _ => DaemonResponse::Error(
                "Request handling not yet implemented for new architecture".to_string(),
            ),
        }
    }

    /// Get the browser manager for the given profile
    async fn get_browser(&mut self, profile: Option<String>) -> Result<&mut BrowserManager> {
        let profile = Profile::from_optional_string(profile.clone());

        // Update last accessed time for custom profiles
        if let Profile::Custom(ref name) = profile
            && let Some(metadata) = self.profile_registry.get_mut(name)
        {
            metadata.last_accessed = Some(chrono::Utc::now());
            // Save registry to disk (ignore errors here to avoid disrupting operations)
            let _ = self.save_profile_registry();
        }

        match profile {
            Profile::Core(CoreProfile::Default) => Ok(self.default_browser()),
            Profile::Core(CoreProfile::OneShot) => Ok(self.oneshot_browser()),
            Profile::Custom(ref name) => {
                // Get or create the profile
                let profile_state = self.get_or_create_profile(name.clone()).await?;
                Ok(&mut profile_state.browser)
            }
        }
    }

    /// Helper to determine if operation is one-shot and generate tab name
    fn prepare_tab_name(tab_name: &str) -> (String, bool) {
        let is_oneshot = tab_name.is_empty();
        let actual_tab_name = if is_oneshot {
            format!("oneshot-{}", uuid::Uuid::new_v4())
        } else {
            tab_name.to_string()
        };
        (actual_tab_name, is_oneshot)
    }

    /// Get browser for the operation based on profile and oneshot status
    async fn get_browser_for_operation(
        &mut self,
        is_oneshot: bool,
        profile: Option<String>,
    ) -> Result<&mut BrowserManager> {
        if is_oneshot {
            Ok(self.oneshot_browser())
        } else {
            self.get_browser(profile).await
        }
    }

    fn generate_auth_token() -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                 abcdefghijklmnopqrstuvwxyz\
                                 0123456789";
        let mut rng = rand::thread_rng();
        let token: String = (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        token
    }

    fn get_token_path() -> Result<PathBuf> {
        let runtime_dir = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .or_else(|| std::env::temp_dir().into())
            .context("Could not determine runtime directory")?;
        Ok(runtime_dir.join("webprobe-daemon.token"))
    }

    fn save_auth_token(token: &str) -> Result<()> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let token_path = Self::get_token_path()?;
        fs::write(&token_path, token)?;

        // Set file permissions to 600 (read/write for owner only)
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&token_path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&token_path, permissions)?;
        }

        Ok(())
    }

    fn save_browser_type(browser_type: &BrowserType) -> Result<()> {
        use std::fs;

        let browser_path = Self::get_browser_type_path()?;
        let browser_str = match browser_type {
            BrowserType::Chrome => "chrome",
            BrowserType::Firefox => "firefox",
        };
        fs::write(&browser_path, browser_str)?;

        // Set file permissions to 600 (read/write for owner only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&browser_path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&browser_path, permissions)?;
        }

        Ok(())
    }

    fn get_browser_type_path() -> Result<PathBuf> {
        let cache_dir = dirs::cache_dir().context("Could not determine cache directory")?;
        Ok(cache_dir.join("webprobe-daemon.browser"))
    }

    pub fn load_auth_token() -> Result<String> {
        let token_path = Self::get_token_path()?;
        std::fs::read_to_string(&token_path)
            .context("Failed to read auth token. Is the daemon running?")
    }

    fn get_socket_path() -> Result<PathBuf> {
        let runtime_dir = dirs::runtime_dir()
            .or_else(dirs::cache_dir)
            .or_else(|| std::env::temp_dir().into())
            .context("Could not determine runtime directory")?;

        Ok(runtime_dir.join("webprobe-daemon.sock"))
    }

    fn get_socket_name() -> Result<Name<'static>> {
        // Use the same path as get_socket_path() for consistency
        let socket_path = Self::get_socket_path()?;
        let path_string = socket_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Socket path is not valid UTF-8"))?
            .to_owned();
        // Leak the string to get 'static lifetime - this is ok since we only create one daemon
        let path_str: &'static str = Box::leak(path_string.into_boxed_str());
        Ok(path_str.to_fs_name::<GenericFilePath>()?)
    }

    pub fn is_running() -> bool {
        if let Ok(name) = Self::get_socket_name() {
            // Just check if we can connect - don't send data to avoid EOF errors
            Stream::connect(name).is_ok()
        } else {
            false
        }
    }

    pub async fn start(self) -> Result<()> {
        // Check if daemon is already running
        if Self::is_running() {
            anyhow::bail!("Daemon is already running");
        }

        // Remove old socket file if it exists
        let socket_path = Self::get_socket_path()?;
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create listener
        let name = Self::get_socket_name()?;
        let listener = ListenerOptions::new().name(name).create_sync()?;
        info!("Daemon listening on {:?}", socket_path);

        // Wrap self in Arc<Mutex> for shared access
        let daemon = Arc::new(Mutex::new(self));

        // Start accepting connections
        Self::run_server(daemon, listener).await
    }

    async fn run_server(daemon: Arc<Mutex<Self>>, listener: Listener) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
        let shutdown_requested = Arc::new(tokio::sync::Mutex::new(false));

        // Set up signal handlers for graceful shutdown
        let shutdown_tx_clone = shutdown_tx.clone();
        tokio::spawn(async move {
            use tokio::signal;

            #[cfg(unix)]
            {
                let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                    .expect("Failed to create SIGTERM handler");
                let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
                    .expect("Failed to create SIGINT handler");

                tokio::select! {
                    _ = sigterm.recv() => {
                        info!("Received SIGTERM, initiating graceful shutdown");
                    }
                    _ = sigint.recv() => {
                        info!("Received SIGINT, initiating graceful shutdown");
                    }
                }
            }

            #[cfg(not(unix))]
            {
                signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
                info!("Received Ctrl+C, initiating graceful shutdown");
            }

            // Trigger shutdown
            if let Err(e) = shutdown_tx_clone.send(()).await {
                warn!("Failed to send shutdown signal: {}", e);
            }

            // Note: We can't call daemon.shutdown() here because it consumes self
            // The shutdown will happen when the daemon is dropped at the end of run()
            info!("Signal handler triggered shutdown");
        });

        loop {
            // Check if shutdown was requested
            {
                let requested = shutdown_requested.lock().await;
                if *requested {
                    info!("Shutdown requested, stopping server");
                    break;
                }
            }

            // Check for shutdown signal or incoming connections
            tokio::select! {
                // Check for shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal, stopping server");
                    break;
                }

                // Wait a bit before checking for connections again
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                    // Try to accept a connection (non-blocking)
                    if let Some(stream) = listener.incoming().next() {
                        match stream {
                            Ok(stream) => {
                                let daemon_clone = Arc::clone(&daemon);
                                let shutdown_tx_clone = shutdown_tx.clone();
                                let shutdown_requested_clone = Arc::clone(&shutdown_requested);
                                // Handle client in a separate task
                                tokio::spawn(async move {
                                    if let Err(e) = Self::handle_client_with_daemon(stream, daemon_clone, shutdown_tx_clone, shutdown_requested_clone).await {
                                        error!("Error handling client: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                error!("Error accepting connection: {}", e);
                            }
                        }
                    }
                }
            }
        }

        // Shutdown the daemon properly
        info!("Shutting down daemon browsers");

        // Wait a bit for any active handlers to finish
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Try to get ownership of the daemon
        match Arc::try_unwrap(daemon) {
            Ok(mutex) => {
                let daemon = mutex.into_inner();
                if let Err(e) = daemon.shutdown().await {
                    error!("Error shutting down daemon: {}", e);
                }
            }
            Err(arc) => {
                // Still have references, try to lock and shutdown anyway
                let daemon = arc.lock().await;
                // Can't move out of daemon, so we'll just rely on Drop
                error!("Unable to get exclusive access to daemon for shutdown, relying on Drop");
                drop(daemon);
            }
        }

        Ok(())
    }

    async fn handle_client_with_daemon(
        mut stream: Stream,
        daemon: Arc<Mutex<Daemon>>,
        shutdown_tx: tokio::sync::mpsc::Sender<()>,
        shutdown_requested: Arc<tokio::sync::Mutex<bool>>,
    ) -> Result<()> {
        // This is the new method that has access to the daemon instance
        // First do authentication as before...
        let auth_token = match Self::load_auth_token() {
            Ok(token) => token.trim().to_string(),
            Err(e) => {
                error!("Failed to load auth token: {}", e);
                let response = DaemonResponse::Error("Internal server error".to_string());
                let response_json = serde_json::to_string(&response)?;
                stream.write_all(response_json.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()?;
                return Ok(());
            }
        };

        // Read and authenticate (same as before)
        let mut request_line = String::new();
        {
            let mut reader = BufReader::new(&mut stream);
            let bytes_read = reader.read_line(&mut request_line)?;

            if bytes_read == 0 || request_line.trim().is_empty() {
                return Ok(());
            }
        }

        if request_line.ends_with('\n') {
            request_line.pop();
        }

        let first_request: DaemonRequest = serde_json::from_str(&request_line)?;

        // Authenticate
        let authenticated = if let DaemonRequest::Authenticate { ref token } = first_request {
            if token.trim() == auth_token.trim() {
                let response = DaemonResponse::Authenticated;
                let response_json = serde_json::to_string(&response)?;
                stream.write_all(response_json.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()?;
                true
            } else {
                let response = DaemonResponse::Error("Invalid authentication token".to_string());
                let response_json = serde_json::to_string(&response)?;
                stream.write_all(response_json.as_bytes())?;
                stream.write_all(b"\n")?;
                stream.flush()?;
                return Ok(());
            }
        } else {
            let response = DaemonResponse::AuthRequired;
            let response_json = serde_json::to_string(&response)?;
            stream.write_all(response_json.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush()?;
            return Ok(());
        };

        if !authenticated {
            return Ok(());
        }

        // Read actual request
        let mut actual_request_line = String::new();
        let bytes_read = {
            let mut reader = BufReader::new(&mut stream);
            reader.read_line(&mut actual_request_line)?
        };

        if bytes_read == 0 || actual_request_line.trim().is_empty() {
            return Ok(());
        }

        if actual_request_line.ends_with('\n') {
            actual_request_line.pop();
        }

        let request: DaemonRequest = serde_json::from_str(&actual_request_line)?;
        info!("Received authenticated request: {:?}", request);

        // Process request - handle shutdown specially
        if matches!(request, DaemonRequest::Shutdown) {
            info!("Daemon shutting down");

            // Send success response before shutting down
            let response = DaemonResponse::Success("Daemon shutting down".to_string());
            let response_json = serde_json::to_string(&response)?;
            stream.write_all(response_json.as_bytes())?;
            stream.write_all(b"\n")?;
            stream.flush()?;

            // Clean up socket file
            if let Ok(socket_path) = Self::get_socket_path()
                && let Err(e) = std::fs::remove_file(&socket_path)
            {
                warn!("Failed to remove socket file: {}", e);
            }

            // Clean up auth token file
            if let Ok(token_path) = Self::get_token_path()
                && let Err(e) = std::fs::remove_file(&token_path)
            {
                warn!("Failed to remove token file: {}", e);
            }

            // Clean up any WebDriver processes started by the daemon
            crate::webdriver_manager::GLOBAL_WEBDRIVER_MANAGER.stop_all();

            // Set shutdown flag and send signal to the server loop
            {
                let mut requested = shutdown_requested.lock().await;
                *requested = true;
            }
            if let Err(e) = shutdown_tx.send(()).await {
                warn!("Failed to send shutdown signal: {}", e);
            }

            // Give client time to receive the response
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            // Exit the process to ensure clean shutdown
            std::process::exit(0);
        }

        // Now we can use the daemon instance!
        let mut daemon = daemon.lock().await;
        let response = daemon.handle_request(request).await;

        // Send response
        let response_json = serde_json::to_string(&response)?;
        stream.write_all(response_json.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        Ok(())
    }
}

/// Client for communicating with the daemon
pub struct DaemonClient;

impl DaemonClient {
    pub fn send_request(request: DaemonRequest) -> Result<DaemonResponse> {
        let name = Daemon::get_socket_name()?;

        // Connect to daemon
        let mut stream =
            Stream::connect(name).context("Failed to connect to daemon. Is it running?")?;

        // Load auth token
        let auth_token = Daemon::load_auth_token()
            .context("Failed to load auth token. Is the daemon running?")?;

        // First, send authentication
        let auth_request = DaemonRequest::Authenticate { token: auth_token };
        let auth_json = serde_json::to_string(&auth_request)?;
        stream.write_all(auth_json.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        // Read auth response
        let mut auth_response_line = String::new();
        {
            let mut reader = BufReader::new(&mut stream);
            reader.read_line(&mut auth_response_line)?;
        }

        let auth_response: DaemonResponse = serde_json::from_str(&auth_response_line)?;
        match auth_response {
            DaemonResponse::Authenticated => {
                // Continue with actual request
            }
            DaemonResponse::AuthRequired => {
                return Err(anyhow::anyhow!("Authentication required but not accepted"));
            }
            DaemonResponse::Error(e) => {
                return Err(anyhow::anyhow!("Authentication failed: {}", e));
            }
            _ => {
                return Err(anyhow::anyhow!("Unexpected authentication response"));
            }
        }

        // Now send the actual request
        let request_json = serde_json::to_string(&request)?;
        stream.write_all(request_json.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        match reader.read_line(&mut response_line) {
            Ok(0) => {
                // EOF received, daemon closed connection without sending response
                anyhow::bail!("Daemon closed connection without sending response");
            }
            Ok(_) => {
                // Got response
                if response_line.is_empty() {
                    anyhow::bail!("Received empty response from daemon");
                }
                let response: DaemonResponse = serde_json::from_str(&response_line).context(
                    format!("Failed to parse daemon response: {}", response_line),
                )?;
                Ok(response)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn is_daemon_running() -> bool {
        Daemon::is_running()
    }
}

#[cfg(test)]
#[path = "../daemon_test.rs"]
mod daemon_test;
