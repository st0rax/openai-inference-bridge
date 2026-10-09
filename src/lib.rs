//! Core library for OpenAI Inference Bridge.
//!
//! The initial HTTP listener, model-list endpoint, and error envelope exist; Chat Completions and browser runtime are not implemented yet.

pub mod api_error;
pub mod brain_backend;
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
