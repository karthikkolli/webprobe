#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_viewport_size_parse_valid() {
        let vp = ViewportSize::parse("1920x1080").unwrap();
        assert_eq!(vp.width, 1920);
        assert_eq!(vp.height, 1080);
    }

    #[test]
    fn test_viewport_size_parse_invalid_format() {
        assert!(ViewportSize::parse("1920").is_err());
        assert!(ViewportSize::parse("1920-1080").is_err());
        assert!(ViewportSize::parse("").is_err());
    }

    #[test]
    fn test_viewport_size_parse_invalid_numbers() {
        assert!(ViewportSize::parse("abcxdef").is_err());
        assert!(ViewportSize::parse("1920xabc").is_err());
    }

    #[test]
    fn test_position_creation() {
        let pos = Position {
            x: 100.0,
            y: 200.0,
            unit: "px".to_string(),
        };
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 200.0);
        assert_eq!(pos.unit, "px");
    }

    #[test]
    fn test_size_creation() {
        let size = Size {
            width: 1920.0,
            height: 1080.0,
            unit: "px".to_string(),
        };
        assert_eq!(size.width, 1920.0);
        assert_eq!(size.height, 1080.0);
        assert_eq!(size.unit, "px");
    }

    #[test]
    fn test_output_format_json() {
        // Test JSON serialization would go here
        let format = OutputFormat::Json;
        assert!(matches!(format, OutputFormat::Json));
    }

    #[test]
    fn test_output_format_simple() {
        let format = OutputFormat::Simple;
        assert!(matches!(format, OutputFormat::Simple));
    }

    #[test]
    fn test_inspection_depth_variants() {
        assert!(matches!(InspectionDepth::Shallow, InspectionDepth::Shallow));
        assert!(matches!(
            InspectionDepth::Children,
            InspectionDepth::Children
        ));
        assert!(matches!(InspectionDepth::Deep, InspectionDepth::Deep));
        assert!(matches!(InspectionDepth::Full, InspectionDepth::Full));
    }

    #[test]
    fn test_element_info_default() {
        let elem = ElementInfo {
            selector: "test".to_string(),
            browser: "Firefox".to_string(),
            position: Position {
                x: 0.0,
                y: 0.0,
                unit: "px".to_string(),
            },
            size: Size {
                width: 100.0,
                height: 50.0,
                unit: "px".to_string(),
            },
            computed_styles: serde_json::Value::Object(serde_json::Map::new()),
            text_content: None,
            children_count: 0,
            metadata: None,
        };

        assert_eq!(elem.selector, "test");
        assert_eq!(elem.browser, "Firefox");
        assert_eq!(elem.children_count, 0);
        assert!(elem.text_content.is_none());
        assert!(elem.metadata.is_none());
    }
}
