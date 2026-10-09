//! Core library for OpenAI Inference Bridge.
//!
//! HTTP API and browser runtime are not implemented yet.

/// Package version embedded at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn package_version_is_not_empty() {
        assert!(!super::VERSION.trim().is_empty());
    }
}
