#[cfg(test)]
mod tests {
    use crate::daemon::*;
    use serde_json::json;

    #[test]
    fn test_daemon_request_serialization() {
        // Test Authenticate request
        let auth_req = DaemonRequest::Authenticate {
            token: "test123".to_string(),
        };
        let json = serde_json::to_string(&auth_req).unwrap();
        assert!(json.contains("Authenticate"));
        assert!(json.contains("test123"));

        // Test CreateTab request
        let create_tab = DaemonRequest::CreateTab {
            name: "test_tab".to_string(),
            browser_type: "chrome".to_string(),
            profile: None,
            viewport: None,
            headless: true,
        };
        let json = serde_json::to_string(&create_tab).unwrap();
        assert!(json.contains("CreateTab"));
        assert!(json.contains("test_tab"));

        // Test Inspect request
        let inspect = DaemonRequest::Inspect {
            tab_name: "tab1".to_string(),
            url: "https://example.com".to_string(),
            selector: ".test".to_string(),
            all: false,
            index: None,
            expect_one: false,
            profile: None,
        };
        let json = serde_json::to_string(&inspect).unwrap();
        assert!(json.contains("Inspect"));
        assert!(json.contains(".test"));
    }

    #[test]
    fn test_daemon_response_serialization() {
        // Test Success response
        let success = DaemonResponse::Success("Operation completed".to_string());
        let json = serde_json::to_string(&success).unwrap();
        assert!(json.contains("Success"));
        assert!(json.contains("Operation completed"));

        // Test Error response
        let error = DaemonResponse::Error("Something went wrong".to_string());
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("Something went wrong"));

        // Test TabList response
        let tabs = vec![TabInfo {
            name: "tab1".to_string(),
            url: Some("https://example.com".to_string()),
            profile: None,
            browser_type: "chrome".to_string(),
            viewport: None,
        }];
        let tab_list = DaemonResponse::TabList(tabs);
        let json = serde_json::to_string(&tab_list).unwrap();
        assert!(json.contains("TabList"));
        assert!(json.contains("tab1"));
    }

    #[test]
    fn test_tab_info_structure() {
        let tab_info = TabInfo {
            name: "my_tab".to_string(),
            url: Some("https://test.com".to_string()),
            profile: Some("custom_profile".to_string()),
            browser_type: "firefox".to_string(),
            viewport: None,
        };

        assert_eq!(tab_info.name, "my_tab");
        assert_eq!(tab_info.url, Some("https://test.com".to_string()));
        assert_eq!(tab_info.browser_type, "firefox");
        assert_eq!(tab_info.profile, Some("custom_profile".to_string()));

        // Test serialization
        let json = serde_json::to_string(&tab_info).unwrap();
        let deserialized: TabInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, tab_info.name);
        assert_eq!(deserialized.profile, tab_info.profile);
    }

    #[test]
    fn test_complex_daemon_requests() {
        // Test Diagnose request
        let diagnose = DaemonRequest::Diagnose {
            tab_name: "diagnose_tab".to_string(),
            url: "https://example.com".to_string(),
            selector: Some(".container".to_string()),
            check_type: "all".to_string(),
            viewport: None,
            profile: None,
        };
        let json = serde_json::to_string(&diagnose).unwrap();
        let deserialized: DaemonRequest = serde_json::from_str(&json).unwrap();
        if let DaemonRequest::Diagnose {
            selector,
            check_type,
            ..
        } = deserialized
        {
            assert_eq!(selector, Some(".container".to_string()));
            assert_eq!(check_type, "all");
        } else {
            panic!("Expected Diagnose request");
        }

        // Test Validate request
        let validate = DaemonRequest::Validate {
            tab_name: "validate_tab".to_string(),
            url: "https://example.com".to_string(),
            check_type: "accessibility".to_string(),
            profile: Some("test_profile".to_string()),
        };
        let json = serde_json::to_string(&validate).unwrap();
        assert!(json.contains("Validate"));
        assert!(json.contains("accessibility"));

        // Test Compare request
        let compare = DaemonRequest::Compare {
            tab_name: "compare_tab".to_string(),
            url1: "https://example1.com".to_string(),
            url2: "https://example2.com".to_string(),
            mode: "visual".to_string(),
            selector: Some(".main".to_string()),
            profile: None,
        };
        let json = serde_json::to_string(&compare).unwrap();
        assert!(json.contains("Compare"));
        assert!(json.contains("example1"));
        assert!(json.contains("example2"));
    }

    #[test]
    fn test_daemon_response_variants() {
        // Test ValidateResult
        let validation_data = json!({
            "score": 85,
            "accessibility": [],
            "seo": [],
            "performance": []
        });
        let validate_result = DaemonResponse::ValidateResult(validation_data.clone());
        let json = serde_json::to_string(&validate_result).unwrap();
        let deserialized: DaemonResponse = serde_json::from_str(&json).unwrap();
        if let DaemonResponse::ValidateResult(data) = deserialized {
            assert_eq!(data["score"], 85);
        } else {
            panic!("Expected ValidateResult");
        }

        // Test DiagnoseResult
        let diagnose_data = json!({
            "issues": [],
            "warnings": [],
            "suggestions": []
        });
        let diagnose_result = DaemonResponse::DiagnoseResult(diagnose_data);
        let json = serde_json::to_string(&diagnose_result).unwrap();
        assert!(json.contains("DiagnoseResult"));

        // Test CompareResult
        let compare_data = json!({
            "differences": [],
            "similarities": [],
            "metrics": {
                "similarity_score": 95.5
            }
        });
        let compare_result = DaemonResponse::CompareResult(compare_data);
        let json = serde_json::to_string(&compare_result).unwrap();
        assert!(json.contains("CompareResult"));
    }

    #[test]
    fn test_screenshot_response() {
        let screenshot_response = DaemonResponse::ScreenshotResult {
            saved_to: "/tmp/screenshot.png".to_string(),
            bytes: 123456,
        };

        let json = serde_json::to_string(&screenshot_response).unwrap();
        let deserialized: DaemonResponse = serde_json::from_str(&json).unwrap();

        if let DaemonResponse::ScreenshotResult { saved_to, bytes } = deserialized {
            assert_eq!(saved_to, "/tmp/screenshot.png");
            assert_eq!(bytes, 123456);
        } else {
            panic!("Expected ScreenshotResult");
        }
    }

    #[test]
    fn test_batch_request() {
        let commands_json = json!([
            {"action": "click", "selector": "button"},
            {"action": "type", "selector": "input", "text": "test"}
        ]);
        let batch = DaemonRequest::Batch {
            tab_name: "batch_tab".to_string(),
            commands: commands_json.to_string(),
            profile: None,
        };

        let json = serde_json::to_string(&batch).unwrap();
        let deserialized: DaemonRequest = serde_json::from_str(&json).unwrap();

        if let DaemonRequest::Batch { commands, .. } = deserialized {
            let parsed_commands: Vec<serde_json::Value> = serde_json::from_str(&commands).unwrap();
            assert_eq!(parsed_commands.len(), 2);
            assert_eq!(parsed_commands[0]["action"], "click");
            assert_eq!(parsed_commands[1]["action"], "type");
        } else {
            panic!("Expected Batch request");
        }
    }

    #[tokio::test]
    #[ignore] // socket_path is not a public method
    async fn test_daemon_client_socket_path() {
        // This test is disabled as socket_path is internal to the daemon
        // The actual socket path is determined by Daemon::get_socket_name()
    }
}
