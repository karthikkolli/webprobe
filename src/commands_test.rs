#[cfg(test)]
mod tests {
    use crate::types::*;

    #[test]
    fn test_output_format_serialization() {
        let json_format = OutputFormat::Json;
        assert!(matches!(json_format, OutputFormat::Json));

        let simple_format = OutputFormat::Simple;
        assert!(matches!(simple_format, OutputFormat::Simple));
    }

    #[test]
    fn test_inspection_depth() {
        let shallow = InspectionDepth::Shallow;
        assert!(matches!(shallow, InspectionDepth::Shallow));

        let children = InspectionDepth::Children;
        assert!(matches!(children, InspectionDepth::Children));

        let deep = InspectionDepth::Deep;
        assert!(matches!(deep, InspectionDepth::Deep));

        let full = InspectionDepth::Full;
        assert!(matches!(full, InspectionDepth::Full));
    }

    #[test]
    fn test_element_info_structure() {
        let element = ElementInfo {
            selector: ".test".to_string(),
            browser: "chrome".to_string(),
            position: Position {
                x: 10.0,
                y: 20.0,
                unit: "px".to_string(),
            },
            size: Size {
                width: 100.0,
                height: 50.0,
                unit: "px".to_string(),
            },
            computed_styles: serde_json::json!({}),
            text_content: Some("Test content".to_string()),
            children_count: 0,
            metadata: None,
        };

        assert_eq!(element.selector, ".test");
        assert_eq!(element.browser, "chrome");
        assert_eq!(element.text_content, Some("Test content".to_string()));
        assert_eq!(element.position.x, 10.0);
        assert_eq!(element.size.width, 100.0);
        assert_eq!(element.children_count, 0);
    }

    #[test]
    fn test_console_message() {
        use crate::webdriver::ConsoleMessage;

        let log_msg = ConsoleMessage {
            level: "log".to_string(),
            message: "Test message".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        assert_eq!(log_msg.level, "log");
        assert_eq!(log_msg.message, "Test message");
        assert_eq!(log_msg.timestamp, "2024-01-01T00:00:00Z");

        // Test serialization
        let json = serde_json::to_string(&log_msg).unwrap();
        let deserialized: ConsoleMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message, log_msg.message);
    }

    #[test]
    fn test_layout_info() {
        use crate::types::{BoundingBox, BoxModel, BoxSides, ContentBox};
        use std::collections::HashMap;

        let layout = LayoutInfo {
            selector: ".container".to_string(),
            tag: "div".to_string(),
            classes: vec!["container".to_string()],
            id: Some("main".to_string()),
            bounds: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
            box_model: BoxModel {
                padding: BoxSides {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                },
                border: BoxSides {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                },
                margin: BoxSides {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 0.0,
                },
                content: ContentBox {
                    width: 1920.0,
                    height: 1080.0,
                },
            },
            computed_styles: HashMap::new(),
            is_visible: true,
            children: vec![],
            warnings: vec![],
            element_count: 10,
            truncated: false,
        };

        assert_eq!(layout.bounds.width, 1920.0);
        assert_eq!(layout.bounds.height, 1080.0);
        assert_eq!(layout.element_count, 10);
        assert!(layout.is_visible);
        assert!(!layout.truncated);
    }

    #[test]
    fn test_text_search_result() {
        let search_result = TextSearchResult {
            selector: ".button".to_string(),
            tag: "button".to_string(),
            text: "Click me".to_string(),
            position: serde_json::json!({"x": 100, "y": 200}),
            size: serde_json::json!({"width": 80, "height": 40}),
            visible: true,
            attributes: serde_json::json!({"id": "btn-1", "class": "btn primary"}),
        };

        assert_eq!(search_result.tag, "button");
        assert_eq!(search_result.text, "Click me");
        assert_eq!(search_result.selector, ".button");
        assert!(search_result.visible);
    }

    #[test]
    fn test_viewport_size() {
        let viewport = ViewportSize {
            width: 1024,
            height: 768,
        };

        assert_eq!(viewport.width, 1024);
        assert_eq!(viewport.height, 768);

        // Test parsing
        let parsed = ViewportSize::parse("800x600").unwrap();
        assert_eq!(parsed.width, 800);
        assert_eq!(parsed.height, 600);

        // Test invalid format
        let invalid = ViewportSize::parse("invalid");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_browser_type_from_str() {
        use crate::webdriver::BrowserType;
        use std::str::FromStr;

        let chrome = BrowserType::from_str("chrome").unwrap();
        assert!(matches!(chrome, BrowserType::Chrome));

        let firefox = BrowserType::from_str("firefox").unwrap();
        assert!(matches!(firefox, BrowserType::Firefox));

        // Test case insensitive
        let chrome_upper = BrowserType::from_str("Chrome").unwrap();
        assert!(matches!(chrome_upper, BrowserType::Chrome));

        let invalid = BrowserType::from_str("safari");
        assert!(invalid.is_err());
    }

    #[test]
    fn test_element_metadata() {
        let metadata = ElementMetadata {
            total_matches: 5,
            returned_index: 0,
            warning: Some("Warning 1".to_string()),
        };

        assert_eq!(metadata.total_matches, 5);
        assert_eq!(metadata.warning, Some("Warning 1".to_string()));

        // Test serialization
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: ElementMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_matches, metadata.total_matches);
        assert_eq!(deserialized.warning, metadata.warning);
    }

    #[test]
    fn test_position_and_size() {
        let pos = Position {
            x: 10.5,
            y: 20.5,
            unit: "px".to_string(),
        };
        assert_eq!(pos.x, 10.5);
        assert_eq!(pos.y, 20.5);

        let size = Size {
            width: 100.0,
            height: 50.0,
            unit: "px".to_string(),
        };
        assert_eq!(size.width, 100.0);
        assert_eq!(size.height, 50.0);

        // Test serialization
        let pos_json = serde_json::to_string(&pos).unwrap();
        assert!(pos_json.contains("10.5"));

        let size_json = serde_json::to_string(&size).unwrap();
        assert!(size_json.contains("100"));
    }
}
