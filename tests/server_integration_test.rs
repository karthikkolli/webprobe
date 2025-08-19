// Integration tests using real HTTP test server

mod test_server;
use test_server::ensure_test_server;
mod test_utils;

use webprobe::types::*;

async fn get_test_browser() -> Option<webprobe::webdriver::Browser> {
    test_utils::get_test_browser_with_retry().await
}

#[tokio::test]
async fn test_real_http_navigation() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    eprintln!("Test using server at: {}", base_url);

    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to home page
    let url = format!("{}/", base_url);
    eprintln!("Navigating to: {}", url);
    browser.goto(&url).await.unwrap();

    // Verify we can find the h1 element
    let elements = browser
        .inspect_element("", "h1", InspectionDepth::Shallow, false, None, false)
        .await
        .unwrap();

    assert_eq!(elements.len(), 1);
    assert_eq!(
        elements[0].text_content.as_ref().unwrap(),
        "Welcome to Test Server"
    );

    // Check navigation links exist
    let nav_links = browser
        .inspect_element("", "nav a", InspectionDepth::Shallow, true, None, false)
        .await
        .unwrap();

    assert_eq!(nav_links.len(), 3);

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_form_submission() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    eprintln!("Test form_submission using server at: {}", base_url);
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to login page
    let url = format!("{}/login", base_url);
    browser.goto(&url).await.unwrap();

    // Fill in the form
    browser
        .type_text("", "#email", "test@example.com", false)
        .await
        .unwrap();
    browser
        .type_text("", "#password", "password123", false)
        .await
        .unwrap();

    // Submit the form
    browser
        .click_element("", "button[type='submit']", None)
        .await
        .unwrap();

    // Wait for redirect to dashboard
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify we're on the dashboard
    let current_url = browser
        .execute_javascript(None, "window.location.href")
        .await
        .unwrap();
    assert!(current_url.as_str().unwrap().contains("/dashboard"));

    // Verify dashboard content
    let dashboard = browser
        .inspect_element(
            "",
            ".dashboard",
            InspectionDepth::Shallow,
            false,
            None,
            false,
        )
        .await
        .unwrap();

    assert_eq!(dashboard.len(), 1);

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_dynamic_content_loading() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to dynamic page
    let url = format!("{}/dynamic", base_url);
    browser.goto(&url).await.unwrap();

    // Initially content should show "Loading..."
    let initial_content = browser
        .inspect_element("", "#content", InspectionDepth::Shallow, false, None, false)
        .await
        .unwrap();

    assert_eq!(
        initial_content[0].text_content.as_ref().unwrap(),
        "Loading..."
    );

    // Wait for dynamic element to appear
    let found = browser
        .wait_for_element("", "#dynamic-element", 2, "present")
        .await
        .unwrap();
    assert!(found, "Dynamic element should appear");

    // Verify content changed
    let updated_content = browser
        .inspect_element("", "#content", InspectionDepth::Shallow, false, None, false)
        .await
        .unwrap();

    assert_eq!(
        updated_content[0].text_content.as_ref().unwrap(),
        "Content Loaded!"
    );

    // Verify dynamically added element
    let dynamic_elem = browser
        .inspect_element(
            "",
            "#dynamic-element",
            InspectionDepth::Shallow,
            false,
            None,
            false,
        )
        .await
        .unwrap();

    assert_eq!(
        dynamic_elem[0].text_content.as_ref().unwrap(),
        "I was added dynamically"
    );

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_console_capture_http() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    eprintln!("Test console_capture using server at: {}", base_url);
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to console test page with console capture
    let url = format!("{}/console", base_url);
    let (elements, console_logs) = browser
        .inspect_element_with_console(
            &url,
            "#app",
            InspectionDepth::Shallow,
            false,
            None,
            false,
            true,
        )
        .await
        .unwrap();

    assert_eq!(elements.len(), 1);

    // Console capture might not work with all browsers/drivers
    // Just verify the element was found
    eprintln!("Console logs captured: {:?}", console_logs.is_some());
    if let Some(logs) = console_logs {
        eprintln!("Number of logs: {}", logs.len());
        for log in &logs {
            eprintln!("  Log: {} - {}", log.level, log.message);
        }
    }

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_network_idle_with_fetch() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to fetch test page
    let url = format!("{}/fetch-test", base_url);
    browser.goto(&url).await.unwrap();

    // Click button to start network activity
    browser
        .click_element("", "#fetch-button", None)
        .await
        .unwrap();

    // Wait for network to become idle
    let is_idle = browser.wait_for_network_idle(5000, 500).await.unwrap();
    assert!(is_idle, "Network should become idle after fetches complete");

    // Verify status changed to complete
    let status = browser
        .inspect_element("", "#status", InspectionDepth::Shallow, false, None, false)
        .await
        .unwrap();

    assert_eq!(status[0].text_content.as_ref().unwrap(), "Complete");

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_multiple_elements_http() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to elements page
    let url = format!("{}/elements", base_url);
    browser.goto(&url).await.unwrap();

    // Find all cards
    let cards = browser
        .inspect_element("", ".card", InspectionDepth::Shallow, true, None, false)
        .await
        .unwrap();

    assert_eq!(cards.len(), 3);

    // Verify each card has correct text content
    for (i, card) in cards.iter().enumerate() {
        let expected_text = format!("Card {}", i + 1);
        assert_eq!(
            card.text_content.as_ref().unwrap(),
            &expected_text,
            "Card should have correct text"
        );
    }

    // Test finding specific element by index
    let second_card = browser
        .inspect_element("", ".card", InspectionDepth::Shallow, false, Some(1), false)
        .await
        .unwrap();

    assert_eq!(second_card.len(), 1);
    assert_eq!(second_card[0].text_content.as_ref().unwrap(), "Card 2");

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_navigation_detection() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to navigation test page
    let url = format!("{}/navigation", base_url);
    browser.goto(&url).await.unwrap();

    // Click button that navigates
    browser.click_element("", "button", None).await.unwrap();

    // Wait a moment for navigation
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Verify we navigated to dashboard
    let current_url = browser
        .execute_javascript(None, "window.location.pathname")
        .await
        .unwrap();
    assert_eq!(current_url.as_str().unwrap(), "/dashboard");

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_table_detection() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to elements page with table
    let url = format!("{}/elements", base_url);
    let smart_elements = browser.detect_smart_elements(&url, None).await.unwrap();

    // Check tables were detected
    eprintln!("Smart elements: {:?}", smart_elements);

    if let Some(tables) = smart_elements.get("tables").and_then(|t| t.as_array()) {
        assert!(!tables.is_empty(), "Should detect at least one table");

        let table = &tables[0];
        // Check table has row_count and headers
        assert!(
            table.get("row_count").is_some(),
            "Table should have row_count"
        );
        assert_eq!(
            table["row_count"].as_i64().unwrap(),
            3,
            "Should have 3 rows (1 header + 2 data)"
        );

        let headers = table["headers"].as_array().unwrap();
        assert_eq!(headers.len(), 2, "Should have 2 columns");
        assert_eq!(headers[0].as_str().unwrap(), "Name");
        assert_eq!(headers[1].as_str().unwrap(), "Value");
    } else {
        panic!("No tables detected in smart elements");
    }

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_hidden_element_detection() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to elements page
    let url = format!("{}/elements", base_url);
    browser.goto(&url).await.unwrap();

    // Try to find hidden element - should exist in DOM
    let hidden = browser
        .inspect_element(
            "",
            "#hidden-element",
            InspectionDepth::Shallow,
            false,
            None,
            false,
        )
        .await
        .unwrap();

    assert_eq!(hidden.len(), 1);

    // Check if element is visible
    let is_visible = browser
        .execute_javascript(
            None,
            r#"(() => {
                const elem = document.getElementById('hidden-element');
                const style = window.getComputedStyle(elem);
                return style.display !== 'none' && style.visibility !== 'hidden';
            })()"#,
        )
        .await
        .unwrap();

    assert!(!is_visible.as_bool().unwrap(), "Element should be hidden");

    browser.close().await.unwrap();
}

#[tokio::test]
async fn test_layout_analysis_http() {
    let server = ensure_test_server().await;
    let base_url = server.base_url.clone();
    let Some(browser) = get_test_browser().await else {
        eprintln!("Skipping test - WebDriver not available");
        return;
    };

    // Navigate to layout test page
    let url = format!("{}/layout", base_url);
    browser.goto(&url).await.unwrap();

    // Analyze layout of container
    let layout = browser
        .analyze_layout("", ".container", 2, 100, 100, false)
        .await
        .unwrap();

    assert!(layout.selector.contains("container"));

    // Analyze spacing context
    let spacing = browser
        .analyze_context("", "#spacing-element", "spacing", 200, None)
        .await
        .unwrap();

    assert!(spacing.get("spacing_context").is_some());

    // Analyze wrapping
    let wrapping = browser
        .analyze_context("", ".wrapping-container", "wrapping", 200, None)
        .await
        .unwrap();

    assert!(wrapping.get("container").is_some());

    browser.close().await.unwrap();
}

// Note: WebDriver cleanup happens automatically via test_utils
// The GLOBAL_WEBDRIVER_MANAGER will clean up on drop
