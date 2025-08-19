use anyhow::Result;
use tracing::info;

use crate::commands::utils;
use crate::daemon::{DaemonClient, DaemonRequest, DaemonResponse};
use crate::types::{DiagnosticResult, OutputFormat};

// Helper function to convert raw analysis data to diagnostic result
fn analyze_to_diagnostic(focus: &str, raw_data: serde_json::Value) -> DiagnosticResult {
    let mut evidence = vec![];
    let confidence;
    let mut suggested_fix = None;

    let diagnosis = match focus {
        "spacing" => {
            // Check for margin collapse
            let mut issues = vec![];

            if let Some(adjacent) = raw_data.get("adjacent_elements").and_then(|a| a.as_array()) {
                for elem in adjacent {
                    if elem
                        .get("margin_collapsed")
                        .and_then(|c| c.as_bool())
                        .unwrap_or(false)
                        && let Some(selector) = elem.get("selector").and_then(|s| s.as_str())
                    {
                        evidence.push(format!("Margin collapse detected on: {}", selector));
                        issues.push("margin collapse");
                    }

                    // Check for excessive margins
                    if let Some(margins) = elem.get("margins").and_then(|m| m.as_object()) {
                        for (side, value) in margins {
                            if let Some(val) = value.as_f64()
                                && val > 100.0
                            {
                                evidence.push(format!("Excessive margin-{}: {}px", side, val));
                                issues.push("excessive margins");
                            }
                        }
                    }
                }
            }

            // Check target element spacing
            if let Some(target) = raw_data.get("target")
                && let Some(box_model) = target.get("box_model")
            {
                // Check for zero padding on containers
                if let Some(padding) = box_model.get("padding").and_then(|p| p.as_object()) {
                    let total_padding: f64 = padding.values().filter_map(|v| v.as_f64()).sum();
                    if total_padding < 1.0 {
                        evidence.push("Container has no padding".to_string());
                        issues.push("missing padding");
                        suggested_fix =
                            Some("Consider adding padding to improve content spacing".to_string());
                    }
                }
            }

            confidence = if issues.is_empty() { 0.95 } else { 0.85 };

            if issues.is_empty() {
                "No spacing issues detected".to_string()
            } else {
                format!("Spacing issues detected: {}", issues.join(", "))
            }
        }

        "wrapping" => {
            let mut issues = vec![];

            // Check overflow indicators
            if let Some(overflow) = raw_data.get("overflow_indicators") {
                if overflow
                    .get("horizontal_overflow")
                    .and_then(|h| h.as_bool())
                    .unwrap_or(false)
                {
                    issues.push("horizontal overflow");
                    evidence.push("Content extends beyond container width".to_string());
                    suggested_fix =
                        Some("Add 'overflow-x: auto' or 'word-wrap: break-word'".to_string());
                }

                if overflow
                    .get("vertical_overflow")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    issues.push("vertical overflow");
                    evidence.push("Content extends beyond container height".to_string());
                }

                if let Some(scroll_width) = overflow.get("scroll_width").and_then(|w| w.as_f64())
                    && let Some(client_width) =
                        overflow.get("client_width").and_then(|w| w.as_f64())
                    && scroll_width > client_width
                {
                    let overflow_amount = scroll_width - client_width;
                    evidence.push(format!(
                        "Content overflows by {}px horizontally",
                        overflow_amount
                    ));
                }
            }

            // Check text overflow
            if let Some(target) = raw_data.get("target")
                && let Some(computed) = target.get("computed_styles")
                && computed.get("white-space").and_then(|w| w.as_str()) == Some("nowrap")
            {
                evidence.push("Text wrapping disabled (white-space: nowrap)".to_string());
                if !issues.contains(&"horizontal overflow") {
                    issues.push("potential text overflow");
                }
            }

            confidence = if issues.is_empty() { 0.90 } else { 0.85 };

            if issues.is_empty() {
                "No wrapping or overflow issues detected".to_string()
            } else {
                format!("Wrapping/overflow issues: {}", issues.join(", "))
            }
        }

        "anomalies" => {
            let mut anomalies = vec![];

            if let Some(target) = raw_data.get("target") {
                // Check visibility
                if let Some(visible) = target.get("visible").and_then(|v| v.as_bool())
                    && !visible
                {
                    anomalies.push("hidden element");
                    evidence.push("Element is not visible to users".to_string());

                    // Try to determine why
                    if let Some(computed) = target.get("computed_styles") {
                        if computed.get("display").and_then(|d| d.as_str()) == Some("none") {
                            evidence.push("Reason: display: none".to_string());
                        } else if computed.get("visibility").and_then(|v| v.as_str())
                            == Some("hidden")
                        {
                            evidence.push("Reason: visibility: hidden".to_string());
                        } else if computed.get("opacity").and_then(|o| o.as_str()) == Some("0") {
                            evidence.push("Reason: opacity: 0".to_string());
                        }
                    }
                }

                // Check z-index issues
                if let Some(computed) = target.get("computed_styles")
                    && let Some(z_index) = computed.get("z-index").and_then(|z| z.as_str())
                    && z_index.parse::<i32>().unwrap_or(0) < -100
                {
                    anomalies.push("extremely low z-index");
                    evidence.push(format!("Element has z-index: {}", z_index));
                }

                // Check dimensions
                if let Some(size) = target.get("size") {
                    let width = size.get("width").and_then(|w| w.as_f64()).unwrap_or(0.0);
                    let height = size.get("height").and_then(|h| h.as_f64()).unwrap_or(0.0);

                    if width < 1.0 || height < 1.0 {
                        anomalies.push("zero dimensions");
                        evidence.push(format!("Element size: {}x{}", width, height));
                    }
                }

                // Check position anomalies
                if let Some(position) = target.get("position") {
                    let x = position.get("x").and_then(|x| x.as_f64()).unwrap_or(0.0);
                    let y = position.get("y").and_then(|y| y.as_f64()).unwrap_or(0.0);

                    if x < -1000.0 || y < -1000.0 {
                        anomalies.push("off-screen positioning");
                        evidence.push(format!("Element positioned at: ({}, {})", x, y));
                        suggested_fix = Some("Check absolute positioning values".to_string());
                    }
                }
            }

            confidence = if anomalies.is_empty() { 0.95 } else { 0.90 };

            if anomalies.is_empty() {
                "No visual anomalies detected".to_string()
            } else {
                format!("Anomalies detected: {}", anomalies.join(", "))
            }
        }

        "alignment" => {
            let mut issues = vec![];

            // Check alignment with adjacent elements
            if let Some(adjacent) = raw_data.get("adjacent_elements").and_then(|a| a.as_array())
                && let Some(target) = raw_data.get("target")
            {
                let target_x = target
                    .get("position")
                    .and_then(|p| p.get("x"))
                    .and_then(|x| x.as_f64())
                    .unwrap_or(0.0);

                for elem in adjacent {
                    let elem_x = elem
                        .get("position")
                        .and_then(|p| p.get("x"))
                        .and_then(|x| x.as_f64())
                        .unwrap_or(0.0);

                    let diff = (target_x - elem_x).abs();

                    // Check for slight misalignment
                    if diff > 0.0
                        && diff < 5.0
                        && let Some(selector) = elem.get("selector").and_then(|s| s.as_str())
                    {
                        issues.push("slight misalignment");
                        evidence.push(format!("{}px misalignment with {}", diff, selector));
                    }
                }
            }

            // Check centering
            if let Some(target) = raw_data.get("target")
                && let Some(computed) = target.get("computed_styles")
            {
                if computed.get("margin-left").and_then(|m| m.as_str()) == Some("auto")
                    && computed.get("margin-right").and_then(|m| m.as_str()) == Some("auto")
                {
                    evidence.push("Element uses auto margins for centering".to_string());
                }

                if let Some(text_align) = computed.get("text-align").and_then(|t| t.as_str())
                    && text_align == "center"
                {
                    evidence.push("Text alignment: center".to_string());
                }
            }

            confidence = if issues.is_empty() { 0.85 } else { 0.75 };

            if issues.is_empty() {
                "Element alignment appears correct".to_string()
            } else {
                format!("Alignment issues: {}", issues.join(", "))
            }
        }

        _ => {
            // Comprehensive analysis - check everything
            let mut all_issues = vec![];

            // Run all checks
            let spacing_result = analyze_to_diagnostic("spacing", raw_data.clone());
            if spacing_result.diagnosis.contains("issues") {
                all_issues.push("spacing");
            }

            let wrapping_result = analyze_to_diagnostic("wrapping", raw_data.clone());
            if wrapping_result.diagnosis.contains("issues") {
                all_issues.push("wrapping");
            }

            let anomaly_result = analyze_to_diagnostic("anomalies", raw_data.clone());
            if anomaly_result.diagnosis.contains("detected")
                && !anomaly_result.diagnosis.contains("No ")
            {
                all_issues.push("anomalies");
            }

            confidence = if all_issues.is_empty() { 0.90 } else { 0.80 };

            if all_issues.is_empty() {
                "Comprehensive analysis complete - no issues detected".to_string()
            } else {
                format!("Issues found in: {}", all_issues.join(", "))
            }
        }
    };

    DiagnosticResult {
        diagnosis,
        confidence,
        evidence,
        suggested_fix,
        raw_data: Some(raw_data),
    }
}

pub async fn handle_analyze(
    url: String,
    selector: String,
    focus: String,
    proximity: u32,
    index: Option<usize>,
    _browser: String,
    profile: Option<String>,
    _no_headless: bool,
    format: OutputFormat,
    tab: Option<String>,
) -> Result<()> {
    info!("Analyzing {} on {} with focus: {}", selector, url, focus);

    // Require daemon for all operations
    utils::require_daemon()?;
    // Create request for daemon
    let request = DaemonRequest::Analyze {
        tab_name: utils::resolve_tab_name(&profile, tab)?,
        url: url.clone(),
        selector: selector.clone(),
        focus: focus.clone(),
        proximity: Some(proximity),
        index,
        profile: profile.clone(),
    };

    // Send request to daemon
    match DaemonClient::send_request(request)? {
        DaemonResponse::AnalyzeResult(result) => {
            // Convert JSON to diagnostic
            let diagnostic = analyze_to_diagnostic(&focus, result);
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&diagnostic)?);
                }
                OutputFormat::Simple => {
                    println!("Diagnosis: {}", diagnostic.diagnosis);
                    println!("Confidence: {:.0}%", diagnostic.confidence * 100.0);
                    if !diagnostic.evidence.is_empty() {
                        println!("\nEvidence:");
                        for evidence in &diagnostic.evidence {
                            println!("  • {}", evidence);
                        }
                    }
                    if let Some(fix) = &diagnostic.suggested_fix {
                        println!("\nSuggested fix: {}", fix);
                    }
                }
            }
            Ok(())
        }
        DaemonResponse::Error(e) => Err(anyhow::anyhow!(e)),
        _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
    }
}
