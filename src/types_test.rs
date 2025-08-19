// Unit tests for types module

use super::*;

#[test]
fn test_viewport_size_parse() {
    // Valid formats
    let size = ViewportSize::parse("1920x1080").unwrap();
    assert_eq!(size.width, 1920);
    assert_eq!(size.height, 1080);

    let size = ViewportSize::parse("800x600").unwrap();
    assert_eq!(size.width, 800);
    assert_eq!(size.height, 600);

    // Invalid formats
    assert!(ViewportSize::parse("1920").is_err());
    assert!(ViewportSize::parse("1920x").is_err());
    assert!(ViewportSize::parse("x1080").is_err());
    assert!(ViewportSize::parse("abc x def").is_err());
    assert!(ViewportSize::parse("1920X1080").is_err()); // uppercase X
}

#[test]
fn test_inspection_depth_values() {
    // Test that enum values are as expected
    let shallow = InspectionDepth::Shallow;
    let children = InspectionDepth::Children;
    let deep = InspectionDepth::Deep;
    let full = InspectionDepth::Full;

    // Ensure they're different
    assert!(!matches!(shallow, InspectionDepth::Children));
    assert!(!matches!(children, InspectionDepth::Deep));
    assert!(!matches!(deep, InspectionDepth::Full));
    assert!(!matches!(full, InspectionDepth::Shallow));
}

#[test]
fn test_output_format() {
    let json = OutputFormat::Json;
    let simple = OutputFormat::Simple;

    // Ensure they're different variants
    assert!(matches!(json, OutputFormat::Json));
    assert!(matches!(simple, OutputFormat::Simple));
    assert!(!matches!(json, OutputFormat::Simple));
    assert!(!matches!(simple, OutputFormat::Json));
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
        width: 300.0,
        height: 400.0,
        unit: "px".to_string(),
    };
    assert_eq!(size.width, 300.0);
    assert_eq!(size.height, 400.0);
    assert_eq!(size.unit, "px");
}

#[test]
fn test_element_metadata() {
    let metadata = ElementMetadata {
        total_matches: 5,
        returned_index: 2,
        warning: Some("Multiple elements found".to_string()),
    };

    assert_eq!(metadata.total_matches, 5);
    assert_eq!(metadata.returned_index, 2);
    assert_eq!(
        metadata.warning,
        Some("Multiple elements found".to_string())
    );

    // Test with no warning
    let metadata2 = ElementMetadata {
        total_matches: 1,
        returned_index: 0,
        warning: None,
    };

    assert_eq!(metadata2.total_matches, 1);
    assert_eq!(metadata2.returned_index, 0);
    assert_eq!(metadata2.warning, None);
}

#[test]
fn test_layout_info_creation() {
    let layout = LayoutInfo {
        selector: ".container".to_string(),
        tag: "div".to_string(),
        id: Some("root".to_string()),
        classes: vec!["container".to_string()],
        bounds: BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
        box_model: BoxModel {
            margin: BoxSides {
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
            padding: BoxSides {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            content: ContentBox {
                width: 100.0,
                height: 100.0,
            },
        },
        computed_styles: HashMap::new(),
        is_visible: true,
        children: vec![],
        warnings: vec![],
        element_count: 1,
        truncated: false,
    };

    assert_eq!(layout.tag, "div");
    assert_eq!(layout.id, Some("root".to_string()));
    assert_eq!(layout.classes, vec!["container".to_string()]);
    assert_eq!(layout.element_count, 1);
    assert!(layout.is_visible);
    assert!(!layout.truncated);
}

#[test]
fn test_bounding_box() {
    let bounds = BoundingBox {
        x: 10.5,
        y: 20.5,
        width: 100.0,
        height: 50.0,
    };

    assert_eq!(bounds.x, 10.5);
    assert_eq!(bounds.y, 20.5);
    assert_eq!(bounds.width, 100.0);
    assert_eq!(bounds.height, 50.0);
}

#[test]
fn test_box_model() {
    let box_model = BoxModel {
        margin: BoxSides {
            top: 10.0,
            right: 10.0,
            bottom: 10.0,
            left: 10.0,
        },
        border: BoxSides {
            top: 1.0,
            right: 1.0,
            bottom: 1.0,
            left: 1.0,
        },
        padding: BoxSides {
            top: 5.0,
            right: 5.0,
            bottom: 5.0,
            left: 5.0,
        },
        content: ContentBox {
            width: 100.0,
            height: 50.0,
        },
    };

    assert_eq!(box_model.margin.top, 10.0);
    assert_eq!(box_model.border.top, 1.0);
    assert_eq!(box_model.padding.top, 5.0);
    assert_eq!(box_model.content.width, 100.0);
    assert_eq!(box_model.content.height, 50.0);
}
