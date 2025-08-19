/// Unit tests for BrowserManager viewport functionality
/// Tests the internal viewport storage and retrieval APIs directly
/// For integration tests that go through the daemon, see viewport_test.rs
use anyhow::Result;

mod test_server;

#[cfg(test)]
mod tests {
    use super::*;
    use webprobe::browser_manager::BrowserManager;
    use webprobe::types::ViewportSize;
    use webprobe::webdriver::BrowserType;

    #[tokio::test]
    async fn test_viewport_storage_and_retrieval() -> Result<()> {
        // Create a browser manager
        let manager = BrowserManager::new(
            BrowserType::Chrome,
            Some("test_profile".to_string()),
            None, // No initial viewport
            true, // headless
        )
        .await?;

        // Create two tabs
        manager.create_tab("desktop_tab".to_string()).await?;
        manager.create_tab("mobile_tab".to_string()).await?;

        // Set different viewports for each tab
        let desktop_viewport = ViewportSize {
            width: 1920,
            height: 1080,
        };
        let mobile_viewport = ViewportSize {
            width: 375,
            height: 812,
        };

        manager
            .set_tab_viewport("desktop_tab", desktop_viewport)
            .await?;
        manager
            .set_tab_viewport("mobile_tab", mobile_viewport)
            .await?;

        // Verify viewports are stored correctly
        let desktop_vp = manager.get_tab_viewport("desktop_tab");
        assert!(desktop_vp.is_some());
        assert_eq!(desktop_vp.unwrap().width, 1920);
        assert_eq!(desktop_vp.unwrap().height, 1080);

        let mobile_vp = manager.get_tab_viewport("mobile_tab");
        assert!(mobile_vp.is_some());
        assert_eq!(mobile_vp.unwrap().width, 375);
        assert_eq!(mobile_vp.unwrap().height, 812);

        // Update viewport for desktop tab
        let new_viewport = ViewportSize {
            width: 1366,
            height: 768,
        };
        manager
            .set_tab_viewport("desktop_tab", new_viewport)
            .await?;

        let updated_vp = manager.get_tab_viewport("desktop_tab");
        assert!(updated_vp.is_some());
        assert_eq!(updated_vp.unwrap().width, 1366);
        assert_eq!(updated_vp.unwrap().height, 768);

        // Mobile viewport should remain unchanged
        let mobile_vp_check = manager.get_tab_viewport("mobile_tab");
        assert!(mobile_vp_check.is_some());
        assert_eq!(mobile_vp_check.unwrap().width, 375);
        assert_eq!(mobile_vp_check.unwrap().height, 812);

        // Clean up
        manager.shutdown().await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_viewport_with_tab_operations() -> Result<()> {
        // Create a browser manager
        let manager = BrowserManager::new(
            BrowserType::Chrome,
            Some("viewport_test".to_string()),
            None,
            true,
        )
        .await?;

        // Create a tab with a specific viewport
        manager.create_tab("responsive_tab".to_string()).await?;

        // Set mobile viewport
        let mobile_viewport = ViewportSize {
            width: 414,
            height: 896,
        };
        manager
            .set_tab_viewport("responsive_tab", mobile_viewport)
            .await?;

        // Verify the viewport is stored correctly
        let stored_viewport = manager.get_tab_viewport("responsive_tab");
        assert!(stored_viewport.is_some(), "Viewport should be stored");
        assert_eq!(stored_viewport.unwrap().width, 414);
        assert_eq!(stored_viewport.unwrap().height, 896);

        // Test viewport retrieval with different tabs
        manager.create_tab("desktop_tab".to_string()).await?;
        let desktop_viewport = ViewportSize {
            width: 1920,
            height: 1080,
        };
        manager
            .set_tab_viewport("desktop_tab", desktop_viewport)
            .await?;

        // Verify both viewports are stored independently
        let mobile_vp = manager.get_tab_viewport("responsive_tab");
        assert_eq!(mobile_vp.unwrap().width, 414);

        let desktop_vp = manager.get_tab_viewport("desktop_tab");
        assert_eq!(desktop_vp.unwrap().width, 1920);

        // Clean up
        manager.shutdown().await?;

        Ok(())
    }
}
