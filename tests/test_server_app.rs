// Common test server application shared between tests and standalone binary

use axum::{
    Form, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{Html, Json, Redirect},
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct AppState {
    sessions: Arc<Mutex<HashMap<String, SessionData>>>,
    form_submissions: Arc<Mutex<Vec<FormData>>>,
}

#[derive(Clone, Debug)]
struct SessionData {
    username: String,
    authenticated: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct FormData {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginQuery {
    redirect: Option<String>,
}

pub async fn create_app() -> Router {
    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        form_submissions: Arc::new(Mutex::new(Vec::new())),
    };

    Router::new()
        // Static pages
        .route("/", get(home_page))
        .route("/test", get(test_page))
        .route("/login", get(login_page).post(handle_login))
        .route("/dashboard", get(dashboard_page))
        .route("/form", get(form_page).post(handle_form))
        // Dynamic content
        .route("/api/data", get(api_data))
        .route("/api/delayed", get(delayed_response))
        .route("/dynamic", get(dynamic_page))
        .route("/console", get(console_test_page))
        // Element testing pages
        .route("/elements", get(elements_page))
        .route("/layout", get(layout_test_page))
        .route("/navigation", get(navigation_page))
        // Network testing
        .route("/slow", get(slow_page))
        .route("/fetch-test", get(fetch_test_page))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// Page handlers

async fn test_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Test Page</title></head>
    <body>
        <h1>Test Page</h1>
        <div id="content">
            <p>This is a test page for oneshot operations.</p>
        </div>
    </body>
    </html>
    "#,
    )
}

async fn home_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Test Home</title></head>
    <body>
        <h1>Welcome to Test Server</h1>
        <nav>
            <a href="/login">Login</a>
            <a href="/form">Form Test</a>
            <a href="/elements">Elements Test</a>
        </nav>
        <div id="content">
            <p>This is the home page content.</p>
        </div>
    </body>
    </html>
    "#,
    )
}

async fn login_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Login</title></head>
    <body>
        <h1>Login Page</h1>
        <form method="POST" action="/login">
            <div>
                <label>Email:</label>
                <input type="email" id="email" name="email" required>
            </div>
            <div>
                <label>Password:</label>
                <input type="password" id="password" name="password" required>
            </div>
            <button type="submit">Sign In</button>
        </form>
    </body>
    </html>
    "#,
    )
}

async fn handle_login(
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
    Form(form): Form<FormData>,
) -> Redirect {
    // Store session
    let mut sessions = state.sessions.lock().await;
    sessions.insert(
        form.email.clone(),
        SessionData {
            username: form.email.clone(),
            authenticated: true,
        },
    );

    // Store form submission
    let mut submissions = state.form_submissions.lock().await;
    submissions.push(form);

    // Redirect to dashboard or specified URL
    let redirect_url = query.redirect.unwrap_or_else(|| "/dashboard".to_string());
    Redirect::to(&redirect_url)
}

async fn dashboard_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Dashboard</title></head>
    <body>
        <h1>Dashboard</h1>
        <div class="dashboard">
            <p>Welcome! You are logged in.</p>
            <div id="user-data">
                <span class="username">User</span>
            </div>
        </div>
    </body>
    </html>
    "#,
    )
}

async fn form_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Form Test</title></head>
    <body>
        <h1>Form Test Page</h1>
        <form id="test-form" method="POST" action="/form">
            <input type="text" name="field1" id="field1" placeholder="Field 1">
            <input type="text" name="field2" id="field2" placeholder="Field 2">
            <textarea name="message" id="message" placeholder="Message"></textarea>
            <select name="option" id="option">
                <option value="opt1">Option 1</option>
                <option value="opt2">Option 2</option>
            </select>
            <button type="submit">Submit</button>
        </form>
    </body>
    </html>
    "#,
    )
}

async fn handle_form() -> StatusCode {
    StatusCode::OK
}

async fn elements_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Elements Test</title>
        <style>
            .card { display: inline-block; margin: 10px; padding: 20px; border: 1px solid #ccc; }
            .hidden { display: none; }
            .nav-item { display: inline-block; margin: 0 10px; }
        </style>
    </head>
    <body>
        <nav>
            <div class="nav-item">Home</div>
            <div class="nav-item">About</div>
            <div class="nav-item">Contact</div>
        </nav>
        
        <div class="container">
            <div class="card" data-id="1">Card 1</div>
            <div class="card" data-id="2">Card 2</div>
            <div class="card" data-id="3">Card 3</div>
        </div>
        
        <div class="hidden" id="hidden-element">Hidden Content</div>
        
        <button id="action-button">Click Me</button>
        <button class="secondary">Secondary Action</button>
        
        <table id="data-table">
            <tr><th>Name</th><th>Value</th></tr>
            <tr><td>Item 1</td><td>100</td></tr>
            <tr><td>Item 2</td><td>200</td></tr>
        </table>
    </body>
    </html>
    "#,
    )
}

async fn dynamic_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Dynamic Content</title></head>
    <body>
        <h1>Dynamic Page</h1>
        <div id="content">Loading...</div>
        <button id="load-more">Load More</button>
        
        <script>
            // Simulate dynamic content loading
            setTimeout(() => {
                document.getElementById('content').innerHTML = 'Content Loaded!';
                
                // Add new element
                const newDiv = document.createElement('div');
                newDiv.id = 'dynamic-element';
                newDiv.textContent = 'I was added dynamically';
                document.body.appendChild(newDiv);
            }, 500);
            
            document.getElementById('load-more').addEventListener('click', () => {
                fetch('/api/data')
                    .then(res => res.json())
                    .then(data => {
                        const div = document.createElement('div');
                        div.className = 'loaded-item';
                        div.textContent = data.message;
                        document.getElementById('content').appendChild(div);
                    });
            });
        </script>
    </body>
    </html>
    "#,
    )
}

async fn console_test_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Console Test</title></head>
    <body>
        <h1>Console Test Page</h1>
        <div id="app">Console Test</div>
        
        <script>
            console.log('Page loaded');
            console.error('Test error message');
            console.warn('Test warning');
            console.info('Test info');
            
            // Test async console logs
            setTimeout(() => {
                console.log('Delayed log message');
            }, 100);
            
            // Test error handling
            try {
                throw new Error('Test exception');
            } catch(e) {
                console.error('Caught error:', e.message);
            }
        </script>
    </body>
    </html>
    "#,
    )
}

async fn layout_test_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Layout Test</title>
        <style>
            .container { width: 1000px; margin: 0 auto; padding: 20px; }
            .row { display: flex; margin-bottom: 20px; }
            .col { flex: 1; padding: 10px; margin: 0 10px; background: #f0f0f0; }
            .spacing-test { margin: 30px 0; padding: 20px; border: 1px solid #ccc; }
            .wrapping-container { width: 600px; }
            .wrapping-item { display: inline-block; width: 200px; height: 100px; margin: 5px; background: #e0e0e0; }
        </style>
    </head>
    <body>
        <div class="container">
            <div class="row">
                <div class="col">Column 1</div>
                <div class="col">Column 2</div>
                <div class="col">Column 3</div>
            </div>
            
            <div class="spacing-test" id="spacing-element">
                Spacing Test Element
            </div>
            
            <div class="wrapping-container">
                <div class="wrapping-item">Item 1</div>
                <div class="wrapping-item">Item 2</div>
                <div class="wrapping-item">Item 3</div>
                <div class="wrapping-item">Item 4</div>
            </div>
        </div>
    </body>
    </html>
    "#,
    )
}

async fn navigation_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Navigation Test</title></head>
    <body>
        <h1>Navigation Test</h1>
        <nav>
            <a href="/">Home</a>
            <a href="/dashboard">Dashboard</a>
            <a href="/login">Login</a>
        </nav>
        
        <button onclick="window.location.href='/dashboard'">Go to Dashboard</button>
        
        <script>
            // Auto-navigate after delay
            setTimeout(() => {
                if (window.location.search === '?auto-nav') {
                    window.location.href = '/dashboard';
                }
            }, 1000);
        </script>
    </body>
    </html>
    "#,
    )
}

async fn fetch_test_page() -> Html<&'static str> {
    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Fetch Test</title></head>
    <body>
        <h1>Network Activity Test</h1>
        <div id="status">Ready</div>
        <button id="fetch-button">Start Fetching</button>
        
        <script>
            document.getElementById('fetch-button').addEventListener('click', async () => {
                document.getElementById('status').textContent = 'Fetching...';
                
                // Multiple fetch requests
                const promises = [
                    fetch('/api/data'),
                    fetch('/api/delayed'),
                    fetch('/api/data?param=1'),
                ];
                
                await Promise.all(promises);
                document.getElementById('status').textContent = 'Complete';
            });
        </script>
    </body>
    </html>
    "#,
    )
}

async fn slow_page() -> Html<&'static str> {
    // Simulate slow loading
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    Html(
        r#"
    <!DOCTYPE html>
    <html>
    <head><title>Slow Page</title></head>
    <body>
        <h1>This page loaded slowly</h1>
    </body>
    </html>
    "#,
    )
}

// API endpoints

async fn api_data() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "message": "API response",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "data": [1, 2, 3, 4, 5]
    }))
}

async fn delayed_response() -> Json<serde_json::Value> {
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    Json(serde_json::json!({
        "message": "Delayed response",
        "delay_ms": 200
    }))
}
