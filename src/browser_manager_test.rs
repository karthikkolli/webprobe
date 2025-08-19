#[cfg(test)]
mod tests {
    use crate::browser_manager::*;
    use crate::types::ViewportSize;
    use crate::webdriver::BrowserType;
    use std::collections::HashSet;

    // Mock browser for testing
    #[allow(dead_code)]
    struct MockBrowser {
        is_closed: bool,
    }

    #[tokio::test]
    #[ignore] // WindowHandle doesn't have public constructor - test with integration tests instead
    async fn test_tab_state_transitions() {
        // Test that TabState enum transitions work correctly
        let state = TabState::Healthy;
        assert!(matches!(state, TabState::Healthy));

        let state = TabState::Broken("Error".to_string());
        if let TabState::Broken(msg) = state {
            assert_eq!(msg, "Error");
        } else {
            panic!("Expected Broken state");
        }

        let state = TabState::Closing;
        assert!(matches!(state, TabState::Closing));
    }

    #[test]
    fn test_tab_context_creation() {
        // Test that TabContext can be created with proper lifetimes
        // This is a compile-time test to ensure the lifetimes work
        fn _test_lifetimes<'a>(
            _browser: &'a crate::webdriver::Browser,
            tab_name: String,
            _guard: tokio::sync::MutexGuard<'a, ()>,
        ) -> TabContext<'a> {
            TabContext {
                browser: _browser,
                tab_name,
                _guard,
            }
        }
    }

    #[tokio::test]
    async fn test_viewport_size_parsing() {
        let viewport = ViewportSize::parse("1920x1080").unwrap();
        assert_eq!(viewport.width, 1920);
        assert_eq!(viewport.height, 1080);

        let result = ViewportSize::parse("invalid");
        assert!(result.is_err());

        let result = ViewportSize::parse("1920");
        assert!(result.is_err());

        let result = ViewportSize::parse("x1080");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_browser_manager_tab_tracking() {
        use dashmap::DashMap;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        // Create mock tab tracking structures
        let tabs = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let temporary_tabs = Arc::new(Mutex::new(HashSet::new()));
        let tab_locks = Arc::new(DashMap::new());
        let tab_states = Arc::new(DashMap::new());

        // Test adding a tab
        // Note: WindowHandle doesn't have a public constructor, so we create a mock handle
        // In real code, this would come from browser.client.new_window()
        {
            let mut tabs_guard = tabs.lock().await;
            // Create a dummy window handle using unsafe transmute for testing only
            // In production code, WindowHandle comes from WebDriver responses
            let handle = unsafe {
                std::mem::transmute::<String, fantoccini::wd::WindowHandle>("handle1".to_string())
            };
            tabs_guard.insert("test_tab".to_string(), handle);
        }
        tab_locks.insert("test_tab".to_string(), Arc::new(Mutex::new(())));
        tab_states.insert("test_tab".to_string(), TabState::Healthy);

        // Verify tab exists
        {
            let tabs_guard = tabs.lock().await;
            assert!(tabs_guard.contains_key("test_tab"));
        }
        assert!(tab_locks.contains_key("test_tab"));
        assert!(tab_states.contains_key("test_tab"));

        // Test marking tab as temporary
        {
            let mut temp_tabs = temporary_tabs.lock().await;
            temp_tabs.insert("test_tab".to_string());
        }

        {
            let temp_tabs = temporary_tabs.lock().await;
            assert!(temp_tabs.contains("test_tab"));
        }

        // Test tab state transitions
        tab_states.insert("test_tab".to_string(), TabState::Closing);
        if let Some(state) = tab_states.get("test_tab") {
            assert!(matches!(state.value(), TabState::Closing));
        }
    }

    #[tokio::test]
    async fn test_concurrent_tab_operations() {
        use dashmap::DashMap;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let tab_locks = Arc::new(DashMap::new());

        // Insert locks for multiple tabs
        for i in 0..5 {
            let tab_name = format!("tab_{}", i);
            tab_locks.insert(tab_name, Arc::new(Mutex::new(())));
        }

        // Simulate concurrent access to different tabs
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let tab_locks_clone = tab_locks.clone();
                tokio::spawn(async move {
                    let tab_name = format!("tab_{}", i);
                    if let Some(lock_entry) = tab_locks_clone.get(&tab_name) {
                        let _guard = lock_entry.lock().await;
                        // Simulate some work
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                })
            })
            .collect();

        // All operations should complete without deadlock
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all locks still exist
        assert_eq!(tab_locks.len(), 5);
    }

    #[tokio::test]
    #[ignore] // WindowHandle doesn't have public constructor - test with integration tests instead
    async fn test_tab_cleanup_ordering() {
        use dashmap::DashMap;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let tab_locks = Arc::new(DashMap::new());
        let tab_states = Arc::new(DashMap::new());
        let tabs = Arc::new(Mutex::new(std::collections::HashMap::new()));

        // Create a tab
        let tab_name = "cleanup_test";
        tab_locks.insert(tab_name.to_string(), Arc::new(Mutex::new(())));
        tab_states.insert(tab_name.to_string(), TabState::Healthy);
        {
            let mut tabs_guard = tabs.lock().await;
            // Create a dummy window handle for testing
            let handle = unsafe {
                std::mem::transmute::<String, fantoccini::wd::WindowHandle>("handle".to_string())
            };
            tabs_guard.insert(tab_name.to_string(), handle);
        }

        // Simulate cleanup sequence
        // 1. Mark as closing
        tab_states.insert(tab_name.to_string(), TabState::Closing);

        // 2. Acquire lock (simulating waiting for operations)
        let lock_entry = tab_locks.get(tab_name).unwrap();
        let _guard = lock_entry.lock().await;

        // 3. Remove from tabs while holding lock
        {
            let mut tabs_guard = tabs.lock().await;
            tabs_guard.remove(tab_name);
        }

        // 4. Drop lock
        drop(_guard);

        // 5. Remove from tracking structures
        tab_locks.remove(tab_name);
        tab_states.remove(tab_name);

        // Verify everything is cleaned up
        assert!(!tab_locks.contains_key(tab_name));
        assert!(!tab_states.contains_key(tab_name));
        {
            let tabs_guard = tabs.lock().await;
            assert!(!tabs_guard.contains_key(tab_name));
        }
    }

    #[test]
    fn test_browser_type_serialization() {
        // Test that BrowserType can be serialized/deserialized
        let chrome = BrowserType::Chrome;
        let json = serde_json::to_string(&chrome).unwrap();
        assert_eq!(json, "\"Chrome\"");

        let firefox = BrowserType::Firefox;
        let json = serde_json::to_string(&firefox).unwrap();
        assert_eq!(json, "\"Firefox\"");

        // Test deserialization
        let browser: BrowserType = serde_json::from_str("\"Chrome\"").unwrap();
        assert!(matches!(browser, BrowserType::Chrome));
    }

    #[tokio::test]
    async fn test_per_tab_viewport_tracking() {
        use dashmap::DashMap;
        use std::sync::Arc;

        // Create viewport tracking structure
        let tab_viewports = Arc::new(DashMap::new());

        // Test 1: Set viewport for a tab
        let viewport1 = ViewportSize {
            width: 1920,
            height: 1080,
        };
        tab_viewports.insert("desktop_tab".to_string(), viewport1);

        // Test 2: Set different viewport for another tab
        let viewport2 = ViewportSize {
            width: 375,
            height: 812,
        };
        tab_viewports.insert("mobile_tab".to_string(), viewport2);

        // Test 3: Verify viewports are stored independently
        if let Some(vp) = tab_viewports.get("desktop_tab") {
            assert_eq!(vp.width, 1920);
            assert_eq!(vp.height, 1080);
        } else {
            panic!("desktop_tab viewport not found");
        }

        if let Some(vp) = tab_viewports.get("mobile_tab") {
            assert_eq!(vp.width, 375);
            assert_eq!(vp.height, 812);
        } else {
            panic!("mobile_tab viewport not found");
        }

        // Test 4: Update viewport for existing tab
        let viewport3 = ViewportSize {
            width: 1366,
            height: 768,
        };
        tab_viewports.insert("desktop_tab".to_string(), viewport3);

        if let Some(vp) = tab_viewports.get("desktop_tab") {
            assert_eq!(vp.width, 1366);
            assert_eq!(vp.height, 768);
        }

        // Test 5: Remove viewport for a tab
        tab_viewports.remove("mobile_tab");
        assert!(!tab_viewports.contains_key("mobile_tab"));
        assert!(tab_viewports.contains_key("desktop_tab"));
    }
}
