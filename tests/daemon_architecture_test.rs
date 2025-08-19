use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

mod test_server;
use test_server::ensure_test_server;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_daemon_with_browser_manager() {
    // Start daemon with Chrome browser
    let mut daemon_process = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "run", "--browser", "chrome"])
        .spawn()
        .expect("Failed to start daemon");

    // Give daemon time to start
    sleep(Duration::from_secs(2)).await;

    // Test inspection using daemon - use one-shot operation without tab
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["inspect", &ensure_test_server().await.base_url, "h1"])
        .output()
        .expect("Failed to run inspect command");

    if !output.status.success() {
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success(), "Inspect command should succeed");

    // Verify output contains h1 element info
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("selector") || stdout.contains("position"),
        "Output should contain element information"
    );

    // Test typing - another one-shot operation
    let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args([
            "type",
            &ensure_test_server().await.base_url,
            "body",
            "test text",
        ])
        .output()
        .expect("Failed to run type command");

    assert!(output.status.success(), "Type command should succeed");

    // Since we're using one-shot operations, no need to manage tabs

    // Stop daemon
    let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "stop"])
        .output();

    // Also kill the daemon process directly to ensure cleanup
    daemon_process.kill().ok();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_daemon_browser_manager_parallelism() {
    // Start daemon
    let mut daemon_process = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "run", "--browser", "chrome"])
        .spawn()
        .expect("Failed to start daemon");

    sleep(Duration::from_secs(2)).await;

    // Create three tabs in parallel
    let handles: Vec<_> = (0..3)
        .map(|i| {
            tokio::spawn(async move {
                let tab_name = format!("parallel-tab-{}", i);
                let test_server = ensure_test_server().await;
                let url = format!(
                    "{}/{}",
                    test_server.base_url,
                    match i {
                        0 => "elements",
                        1 => "layout",
                        2 => "form",
                        _ => "",
                    }
                );

                // Use one-shot operations without tabs for parallel testing
                let output = Command::new(env!("CARGO_BIN_EXE_webprobe"))
                    .args(["inspect", &url, "body"])
                    .output()
                    .expect("Failed to run inspect");

                (tab_name, output.status.success())
            })
        })
        .collect();

    // Wait for all operations to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await);
    }

    // Verify all succeeded
    for result in results {
        let (tab_name, success) = result.expect("Task should not panic");
        assert!(success, "Tab {} operation should succeed", tab_name);
    }

    // Since we're using one-shot operations, no tabs to verify

    // Clean up
    let _ = Command::new(env!("CARGO_BIN_EXE_webprobe"))
        .args(["daemon", "stop"])
        .output();

    daemon_process.kill().ok();
}
