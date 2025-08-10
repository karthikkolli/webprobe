use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::types::ViewportSize;
use crate::webdriver::{Browser, BrowserType};

/// Manages persistent browser tabs across commands
pub struct TabManager {
    tabs: Arc<Mutex<HashMap<String, Arc<Mutex<PersistentTab>>>>>,
}

pub struct PersistentTab {
    pub name: String,
    pub browser: Browser,
    pub profile: Option<String>,
    pub current_url: Option<String>,
    pub created_at: Instant,
    pub last_used: Instant,
}

impl TabManager {
    /// Create a new tab manager
    pub fn new() -> Self {
        Self {
            tabs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get or create a tab with the given name
    pub async fn get_or_create_tab(
        &self,
        name: &str,
        browser_type: BrowserType,
        profile: Option<String>,
        viewport: Option<ViewportSize>,
        headless: bool,
    ) -> Result<Arc<Mutex<PersistentTab>>> {
        let mut tabs = self.tabs.lock().await;

        if let Some(tab) = tabs.get(name) {
            // Update last used time
            let mut tab_lock = tab.lock().await;
            tab_lock.last_used = Instant::now();
            drop(tab_lock);
            Ok(Arc::clone(tab))
        } else {
            // Create new browser instance
            let browser = Browser::new(browser_type, profile.clone(), viewport, headless).await?;

            let tab = PersistentTab {
                name: name.to_string(),
                browser,
                profile,
                current_url: None,
                created_at: Instant::now(),
                last_used: Instant::now(),
            };

            let tab_arc = Arc::new(Mutex::new(tab));
            tabs.insert(name.to_string(), Arc::clone(&tab_arc));
            Ok(tab_arc)
        }
    }

    /// Update the current URL for a tab
    pub async fn update_tab_url(&self, name: &str, url: &str) -> Result<()> {
        let tabs = self.tabs.lock().await;
        if let Some(tab) = tabs.get(name) {
            let mut tab_lock = tab.lock().await;
            tab_lock.current_url = Some(url.to_string());
            tab_lock.last_used = Instant::now();
        }
        Ok(())
    }

    /// Get the current URL for a tab
    pub async fn get_tab_url(&self, name: &str) -> Option<String> {
        let tabs = self.tabs.lock().await;
        if let Some(tab) = tabs.get(name) {
            let tab_lock = tab.lock().await;
            tab_lock.current_url.clone()
        } else {
            None
        }
    }

    /// Check if we need to navigate (URL is different from current)
    pub async fn should_navigate(&self, name: &str, new_url: &str) -> bool {
        if let Some(current_url) = self.get_tab_url(name).await {
            // Only navigate if the base URL is different (ignore fragments and query params for now)
            let current_base = current_url
                .split('?')
                .next()
                .unwrap_or(&current_url)
                .split('#')
                .next()
                .unwrap_or(&current_url);
            let new_base = new_url
                .split('?')
                .next()
                .unwrap_or(new_url)
                .split('#')
                .next()
                .unwrap_or(new_url);
            current_base != new_base
        } else {
            // No current URL, we need to navigate
            true
        }
    }

    /// List all active tabs
    pub async fn list_tabs(&self) -> Vec<TabInfo> {
        self.list_tabs_by_profile(None).await
    }

    /// List tabs filtered by profile
    pub async fn list_tabs_by_profile(&self, profile: Option<&str>) -> Vec<TabInfo> {
        let tabs = self.tabs.lock().await;
        let mut result = Vec::new();
        for tab_arc in tabs.values() {
            let tab = tab_arc.lock().await;
            // If profile filter is specified, only include matching tabs
            if let Some(filter_profile) = profile
                && tab.profile.as_deref() != Some(filter_profile)
            {
                continue;
            }
            result.push(TabInfo {
                name: tab.name.clone(),
                url: tab.current_url.clone(),
                profile: tab.profile.clone(),
                age_seconds: tab.created_at.elapsed().as_secs(),
                last_used_seconds: tab.last_used.elapsed().as_secs(),
            });
        }
        result
    }

    /// Close a specific tab
    pub async fn close_tab(&self, name: &str) -> Result<bool> {
        let mut tabs = self.tabs.lock().await;
        if tabs.remove(name).is_some() {
            // Browser will be dropped when the tab is dropped
            // WebDriver will clean up the browser session
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Close all tabs
    pub async fn close_all_tabs(&self) -> Result<usize> {
        let mut tabs = self.tabs.lock().await;
        let count = tabs.len();

        // Clear all tabs - browsers will be dropped
        tabs.clear();

        Ok(count)
    }
}

#[derive(Debug, Clone)]
pub struct TabInfo {
    pub name: String,
    pub url: Option<String>,
    pub profile: Option<String>,
    #[allow(dead_code)]
    pub age_seconds: u64,
    #[allow(dead_code)]
    pub last_used_seconds: u64,
}

// Global tab manager instance
lazy_static::lazy_static! {
    pub static ref GLOBAL_TAB_MANAGER: TabManager = TabManager::new();
}

#[cfg(test)]
#[path = "tab_manager_test.rs"]
mod tab_manager_test;
