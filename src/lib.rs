//! Core library for OpenAI Inference Bridge.
//!
//! The HTTP listener, model-list endpoint, Chat Completions validation, and Chromium/Edge DevTools driver are implemented. Live provider inference, streaming, and token accounting remain unverified or unsupported.

pub mod api_error;
pub mod brain_backend;
pub mod browser_driver;
pub mod brain_manager;
pub mod brain_registry;
pub mod chat_completion;
pub mod config;
pub mod http_server;
pub mod json;

/// Package version embedded at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn package_version_is_not_empty() {
        assert!(!super::VERSION.trim().is_empty());
    }
}
