// Test web server for integration tests

use std::net::SocketAddr;
use tokio::sync::OnceCell;

// Include the test server app inline
include!("test_server_app.rs");

static TEST_SERVER: OnceCell<TestServerHandle> = OnceCell::const_new();

pub struct TestServerHandle {
    pub addr: SocketAddr,
    pub base_url: String,
}

/// Start the test server once for all tests
pub async fn ensure_test_server() -> &'static TestServerHandle {
    TEST_SERVER
        .get_or_init(|| async {
            // Get a free port first
            let std_listener = std::net::TcpListener::bind("127.0.0.1:0")
                .expect("Failed to bind test server");
            let addr = std_listener.local_addr().unwrap();
            let base_url = format!("http://{}", addr);
            // Close the listener so the thread can bind to it
            drop(std_listener);

            // Spawn the server in a dedicated thread with its own runtime
            let addr_clone = addr;
            let server_handle = std::thread::spawn(move || {
                let runtime = tokio::runtime::Runtime::new()
                    .expect("Failed to create runtime");

                runtime.block_on(async {
                    // Create everything inside this runtime
                    let listener = tokio::net::TcpListener::bind(addr_clone)
                        .await
                        .expect("Failed to bind in thread");
                    let app = create_app().await;
                    axum::serve(listener, app)
                        .await
                        .expect("Test server failed");
                });
            });

            // Wait for server to be ready by actually checking HTTP response
            for i in 0..30 {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                // Use curl to check if server is responding to HTTP requests
                let curl_check = std::process::Command::new("curl")
                    .args(["-s", "-I", "--max-time", "1", &base_url])
                    .output();

                if let Ok(output) = curl_check
                    && output.status.success() {
                        let response = String::from_utf8_lossy(&output.stdout);
                        if response.contains("HTTP/1.1 200") || response.contains("HTTP/1.1") {
                            eprintln!("Test server ready at {} after {} attempts", base_url, i + 1);
                            break;
                        }
                    }

                if i == 29 {
                    panic!("Test server failed to start after 30 attempts - not responding to HTTP requests");
                }
            }

            // We don't join the thread - let it run for the duration of the tests
            // The thread will be killed when the test process exits
            drop(server_handle);

            TestServerHandle { addr, base_url }
        })
        .await
}
