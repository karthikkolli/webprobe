// Integration tests for webprobe

mod common;

use webprobe::types::*;
use webprobe::webdriver::{Browser, BrowserType};

// Note: These tests require geckodriver or chromedriver to be installed
// They will be skipped if drivers are not available

#[tokio::test]
#[ignore] // Ignore by default as it requires WebDriver
async fn test_browser_connection() {
    // Try to connect to browser
    let browser = Browser::new(
        BrowserType::Firefox,
        None,
        None,
        true, // headless
    )
    .await;

    if browser.is_err() {
        eprintln!("Skipping test - WebDriver not available");
        return;
    }

    let browser = browser.unwrap();

    // Navigate to a test page
    let test_html = common::create_test_html(common::fixtures::SIMPLE_PAGE);
    let url = format!("file://{}", test_html.display());

    browser.goto(&url).await.unwrap();

    // Inspect an element
    let results = browser
        .inspect_element("", "h1", InspectionDepth::Shallow, false, None, false)
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].selector, "h1");
    assert_eq!(results[0].text_content.as_deref(), Some("Test Header"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore] // Ignore by default as it requires WebDriver
async fn test_console_capture() {
    let browser = Browser::new(
        BrowserType::Chrome,
        None,
        None,
        true, // headless
    )
    .await;

    if browser.is_err() {
        eprintln!("Skipping test - WebDriver not available");
        return;
    }

    let browser = browser.unwrap();

    // Navigate to page with console logs
    let test_html = common::create_test_html(common::fixtures::PAGE_WITH_CONSOLE);
    let url = format!("file://{}", test_html.display());

    // Navigate and wait for delayed logs to fire
    browser.goto(&url).await.unwrap();

    // Wait for the setTimeout logs to fire
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let (elements, console_logs) = browser
        .inspect_element_with_console(
            "", // Already navigated
            "#app",
            InspectionDepth::Shallow,
            false,
            None,
            false,
            true, // capture console
        )
        .await
        .unwrap();

    assert_eq!(elements.len(), 1);

    if let Some(logs) = console_logs {
        // Should have captured at least some logs
        assert!(!logs.is_empty());

        // Check for expected log types
        let has_log = logs.iter().any(|l| l.level == "log");
        let has_error = logs.iter().any(|l| l.level == "error");
        let has_warn = logs.iter().any(|l| l.level == "warn");

        assert!(has_log || has_error || has_warn);
    }

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore] // Ignore by default as it requires WebDriver
async fn test_multiple_elements() {
    let browser = Browser::new(
        BrowserType::Firefox,
        None,
        None,
        true, // headless
    )
    .await;

    if browser.is_err() {
        eprintln!("Skipping test - WebDriver not available");
        return;
    }

    let browser = browser.unwrap();

    let html = r#"
    <!DOCTYPE html>
    <html>
    <body>
        <div class="item">Item 1</div>
        <div class="item">Item 2</div>
        <div class="item">Item 3</div>
    </body>
    </html>
    "#;

    let test_html = common::create_test_html(html);
    let url = format!("file://{}", test_html.display());

    // Test getting all elements
    let results = browser
        .inspect_element(
            &url,
            ".item",
            InspectionDepth::Shallow,
            true, // all
            None,
            false,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].text_content.as_deref(), Some("Item 1"));
    assert_eq!(results[1].text_content.as_deref(), Some("Item 2"));
    assert_eq!(results[2].text_content.as_deref(), Some("Item 3"));

    // Test getting specific index
    let results = browser
        .inspect_element(
            "",
            ".item",
            InspectionDepth::Shallow,
            false,
            Some(1), // index 1
            false,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text_content.as_deref(), Some("Item 2"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore] // Ignore by default as it requires WebDriver
async fn test_click_element() {
    let browser = Browser::new(
        BrowserType::Chrome,
        None,
        None,
        true, // headless
    )
    .await;

    if browser.is_err() {
        eprintln!("Skipping test - WebDriver not available");
        return;
    }

    let browser = browser.unwrap();

    let html = r#"
    <!DOCTYPE html>
    <html>
    <body>
        <button id="btn" onclick="this.textContent='Clicked'">Click me</button>
    </body>
    </html>
    "#;

    let test_html = common::create_test_html(html);
    let url = format!("file://{}", test_html.display());

    browser.goto(&url).await.unwrap();

    // Click the button
    browser.click_element("", "#btn", None).await.unwrap();

    // Check that text changed
    let results = browser
        .inspect_element("", "#btn", InspectionDepth::Shallow, false, None, false)
        .await
        .unwrap();

    assert_eq!(results[0].text_content.as_deref(), Some("Clicked"));

    browser.close().await.unwrap();
}

#[tokio::test]
#[ignore] // Ignore by default as it requires WebDriver
async fn test_type_text() {
    let browser = Browser::new(
        BrowserType::Firefox,
        None,
        None,
        true, // headless
    )
    .await;

    if browser.is_err() {
        eprintln!("Skipping test - WebDriver not available");
        return;
    }

    let browser = browser.unwrap();

    let html = r#"
    <!DOCTYPE html>
    <html>
    <body>
        <input id="input" type="text" value="initial">
    </body>
    </html>
    "#;

    let test_html = common::create_test_html(html);
    let url = format!("file://{}", test_html.display());

    browser.goto(&url).await.unwrap();

    // Type text with clear
    browser
        .type_text("", "#input", "new text", true)
        .await
        .unwrap();

    // Check value
    let script = "return document.getElementById('input').value";
    let result = browser.execute(script, vec![]).await.unwrap();
    let value: String = serde_json::from_value(result).unwrap();

    assert_eq!(value, "new text");

    browser.close().await.unwrap();
}
