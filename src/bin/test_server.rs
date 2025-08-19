// Standalone test server binary

use std::net::SocketAddr;
use tracing::{Level, info};
use tracing_subscriber;

// Include the shared test server module
include!("../../tests/test_server_app.rs");

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let app = create_app().await;

    // Parse port from args or use default
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");

    info!("Test server listening on http://{}", addr);

    axum::serve(listener, app).await.expect("Server failed");
}
