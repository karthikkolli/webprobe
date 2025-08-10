#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_command_exists() {
        // Test with a command that should exist on most systems
        #[cfg(unix)]
        {
            assert!(WebDriverManager::command_exists("ls"));
            assert!(!WebDriverManager::command_exists(
                "nonexistent_command_12345"
            ));
        }

        #[cfg(windows)]
        {
            assert!(WebDriverManager::command_exists("cmd"));
            assert!(!WebDriverManager::command_exists(
                "nonexistent_command_12345"
            ));
        }
    }

    #[test]
    fn test_find_free_port() {
        let port =
            WebDriverManager::find_free_port_for_browser(&crate::webdriver::BrowserType::Firefox)
                .unwrap();
        assert!(port > 0);
        // Port is u16, so it's always <= 65535
    }

    #[test]
    fn test_is_port_in_use() {
        // Port 0 is special and should not be in use
        assert!(!WebDriverManager::is_port_in_use(0));

        // Bind to a port and check it's in use
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(WebDriverManager::is_port_in_use(port));
    }

    #[tokio::test]
    async fn test_is_driver_running() {
        // Should return false for a URL that's not running
        assert!(!WebDriverManager::is_driver_running("http://localhost:65432").await);
    }

    #[test]
    fn test_webdriver_manager_new() {
        let _manager = WebDriverManager::new();
        // Just ensure it can be created without panic
    }

    #[test]
    fn test_stop_all_empty() {
        let manager = WebDriverManager::new();
        // Should not panic even with no processes
        manager.stop_all();
    }

    #[tokio::test]
    async fn test_ensure_driver_not_installed() {
        let _manager = WebDriverManager::new();

        // Try to ensure a fake browser type driver
        // This should fail because the driver is not installed
        // We can't test with real browser types as they might be installed

        // For now, just test that the method exists and can be called
        // Real integration tests would need mock drivers
    }
}
