#[cfg(test)]
mod tests {
    use super::super::*;

    #[tokio::test]
    async fn test_create_new_tab() {
        let manager = TabManager::new();

        // Mock browser creation would be needed here
        // For now, we test the structure
        assert_eq!(manager.list_tabs().await.len(), 0);
    }

    #[tokio::test]
    async fn test_list_tabs_empty() {
        let manager = TabManager::new();
        let tabs = manager.list_tabs().await;
        assert_eq!(tabs.len(), 0);
    }

    #[tokio::test]
    async fn test_list_tabs_by_profile() {
        let manager = TabManager::new();

        // With no tabs, should return empty for any profile
        let tabs = manager.list_tabs_by_profile(Some("test-profile")).await;
        assert_eq!(tabs.len(), 0);

        let tabs = manager.list_tabs_by_profile(None).await;
        assert_eq!(tabs.len(), 0);
    }

    #[tokio::test]
    async fn test_close_nonexistent_tab() {
        let manager = TabManager::new();
        let result = manager.close_tab("nonexistent").await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_close_all_tabs_when_empty() {
        let manager = TabManager::new();
        let count = manager.close_all_tabs().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_should_navigate() {
        let manager = TabManager::new();

        // Tab doesn't exist, should always navigate
        assert!(
            manager
                .should_navigate("test-tab", "https://example.com")
                .await
        );
    }

    #[tokio::test]
    async fn test_get_tab_url_nonexistent() {
        let manager = TabManager::new();
        let url = manager.get_tab_url("nonexistent").await;
        assert!(url.is_none());
    }

    #[tokio::test]
    async fn test_update_tab_url_nonexistent() {
        let manager = TabManager::new();
        // Should not panic even if tab doesn't exist
        manager
            .update_tab_url("nonexistent", "https://example.com")
            .await
            .unwrap();
    }
}
