use std::fmt;

/// Custom error type that includes exit codes
#[derive(Debug)]
pub enum WebprobeError {
    /// Element not found (exit code 2)
    ElementNotFound(String),
    /// Multiple elements when expecting one (exit code 3)
    MultipleElements { selector: String, count: usize },
    /// WebDriver connection failed (exit code 4)
    WebDriverFailed(String),
    /// Operation timeout (exit code 5)
    Timeout(String),
    /// Generic error (exit code 1)
    Other(anyhow::Error),
}

impl WebprobeError {
    /// Get the exit code for this error
    pub fn exit_code(&self) -> i32 {
        match self {
            WebprobeError::ElementNotFound(_) => 2,
            WebprobeError::MultipleElements { .. } => 3,
            WebprobeError::WebDriverFailed(_) => 4,
            WebprobeError::Timeout(_) => 5,
            WebprobeError::Other(_) => 1,
        }
    }
}

impl fmt::Display for WebprobeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WebprobeError::ElementNotFound(selector) => {
                write!(f, "No elements found matching selector: {}", selector)
            }
            WebprobeError::MultipleElements { selector, count } => {
                write!(
                    f,
                    "Expected exactly one element matching '{}', but found {}",
                    selector, count
                )
            }
            WebprobeError::WebDriverFailed(msg) => {
                write!(f, "WebDriver connection failed: {}", msg)
            }
            WebprobeError::Timeout(msg) => {
                write!(f, "Operation timed out: {}", msg)
            }
            WebprobeError::Other(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for WebprobeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WebprobeError::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for WebprobeError {
    fn from(err: anyhow::Error) -> Self {
        // Try to detect specific error types from the error message
        let msg = err.to_string();

        if msg.contains("No elements found matching selector") {
            WebprobeError::ElementNotFound(msg)
        } else if msg.contains("Expected exactly one element") {
            // Try to parse the count
            if let Some(count_str) = msg.split("found ").nth(1)
                && let Some(count_str) = count_str.split_whitespace().next()
                && let Ok(count) = count_str.parse::<usize>()
            {
                return WebprobeError::MultipleElements {
                    selector: "unknown".to_string(),
                    count,
                };
            }
            WebprobeError::Other(err)
        } else if msg.contains("Failed to connect to WebDriver")
            || msg.contains("WebDriver")
            || msg.contains("geckodriver")
            || msg.contains("chromedriver")
        {
            WebprobeError::WebDriverFailed(msg)
        } else if msg.contains("timeout") || msg.contains("timed out") {
            WebprobeError::Timeout(msg)
        } else {
            WebprobeError::Other(err)
        }
    }
}
