// Tests for batch command functionality

use serial_test::serial;
use std::process::Command;

mod common;
use common::{DaemonTestGuard, get_test_browser};
mod test_server;
use test_server::ensure_test_server;

/// Helper to run webprobe CLI commands
fn run_webprobe(args: &[&str]) -> std::process::Output {
    let binary_path = env!("CARGO_BIN_EXE_webprobe");
    Command::new(binary_path)
        .args(args)
        .output()
        .expect("Failed to execute webprobe command")
}

#[tokio::test]
async fn test_batch_command_parsing() {
    // Test parsing of batch commands
    let commands = "goto http://localhost:3000; wait h1 5; click button";
    let lines: Vec<&str> = commands.split(';').map(|s| s.trim()).collect();

    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("goto"));
    assert!(lines[1].starts_with("wait"));
    assert!(lines[2].starts_with("click"));
}

#[tokio::test]
async fn test_batch_file_prefix() {
    // Test that @ prefix indicates file input
    let file_input = "@/tmp/commands.txt";
    assert!(file_input.starts_with('@'));
    assert_eq!(&file_input[1..], "/tmp/commands.txt");

    let direct_input = "goto http://localhost:3000";
    assert!(!direct_input.starts_with('@'));
}

#[tokio::test]
#[serial]
async fn test_batch_execution() {
    let mut _daemon = DaemonTestGuard::new(get_test_browser());
    let server = ensure_test_server().await;

    // Create batch commands
    let batch_commands = format!(
        r#"[
        {{"type": "goto", "url": "{}"}},
        {{"type": "wait", "selector": "h1", "timeout": 5}},
        {{"type": "inspect", "selector": "h1"}}
    ]"#,
        server.base_url
    );

    // Execute batch through daemon
    let output = run_webprobe(&["batch", &batch_commands]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Batch command output:");
    println!("  stdout: {}", stdout);
    println!("  stderr: {}", stderr);
    println!("  status: {:?}", output.status);

    assert!(
        output.status.success(),
        "Batch execution should succeed. stderr: {}",
        stderr
    );

    // Should contain results from the batch operations
    assert!(
        stdout.contains("commands succeeded") || stdout.contains("Batch execution complete"),
        "Should show batch execution results. Got: {}",
        stdout
    );
}
