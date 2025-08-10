use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Depth of element inspection
#[derive(Clone, Copy, Debug, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum InspectionDepth {
    /// Basic element information only
    Shallow,
    /// Include direct children
    Children,
    /// Include children and grandchildren
    Deep,
    /// Include entire subtree
    Full,
}

/// Output format for CLI results
#[derive(Clone, Copy, Debug, Deserialize, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// JSON format for programmatic consumption
    Json,
    /// Human-readable simple format
    Simple,
}

/// Complete information about a web element
#[derive(Debug, Serialize, Deserialize)]
pub struct ElementInfo {
    /// CSS selector used to find this element
    pub selector: String,
    /// Browser type (Firefox, Chrome)
    pub browser: String,
    /// Element position on the page
    pub position: Position,
    /// Element dimensions
    pub size: Size,
    /// All computed CSS styles as JSON
    pub computed_styles: serde_json::Value,
    /// Text content if available
    pub text_content: Option<String>,
    /// Number of child elements
    pub children_count: usize,
    /// Metadata about element selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ElementMetadata>,
}

/// Metadata about element selection when multiple matches exist
#[derive(Debug, Serialize, Deserialize)]
pub struct ElementMetadata {
    /// Total number of elements matching the selector
    pub total_matches: usize,
    /// Index of the returned element (0-based)
    pub returned_index: usize,
    /// Warning message if multiple elements found
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Position of an element on the page
#[derive(Debug, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate
    pub x: f64,
    /// Y coordinate  
    pub y: f64,
    /// Unit of measurement (typically "px")
    pub unit: String,
}

/// Size dimensions of an element
#[derive(Debug, Serialize, Deserialize)]
pub struct Size {
    /// Width of the element
    pub width: f64,
    /// Height of the element
    pub height: f64,
    /// Unit of measurement (typically "px")
    pub unit: String,
}

/// Browser viewport dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportSize {
    /// Viewport width in pixels
    pub width: u32,
    /// Viewport height in pixels
    pub height: u32,
}

impl ViewportSize {
    /// Parse viewport size from "WIDTHxHEIGHT" format (e.g., "1920x1080")
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split('x').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid viewport format. Use WIDTHxHEIGHT (e.g., 1920x1080)");
        }

        let width = parts[0]
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("Invalid width in viewport size"))?;
        let height = parts[1]
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("Invalid height in viewport size"))?;

        Ok(ViewportSize { width, height })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoxModel {
    pub margin: BoxSides,
    pub border: BoxSides,
    pub padding: BoxSides,
    pub content: ContentBox,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoxSides {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContentBox {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LayoutInfo {
    pub selector: String,
    pub tag: String,
    pub classes: Vec<String>,
    pub id: Option<String>,
    pub bounds: BoundingBox,
    pub box_model: BoxModel,
    pub computed_styles: HashMap<String, String>,
    pub is_visible: bool,
    pub children: Vec<LayoutInfo>,
    pub warnings: Vec<String>,
    pub element_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[cfg(test)]
#[path = "types_test.rs"]
mod types_test;
