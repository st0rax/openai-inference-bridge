//! OpenAI-compatible structured error envelopes for HTTP responses.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub error_type: String,
    pub message: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

impl ApiError {
    pub fn new(status: u16, error_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            error_type: error_type.into(),
            message: message.into(),
            param: None,
            code: None,
        }
    }

    pub fn with_param(mut self, param: impl Into<String>) -> Self {
        self.param = Some(param.into());
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn from_backend(error: &BackendError) -> Self {
        let (status, error_type, code) = match error.kind {
            BackendErrorKind::LoginRequired => {
                (401, "authentication_error", "provider_login_required")
            }
            BackendErrorKind::Challenge => (503, "server_error", "provider_challenge_required"),
            BackendErrorKind::RateLimited => (429, "rate_limit_error", "provider_rate_limited"),
            BackendErrorKind::Timeout => (504, "timeout_error", "brain_timeout"),
            BackendErrorKind::Cancelled => (408, "timeout_error", "request_cancelled"),
            BackendErrorKind::BrowserUnavailable => (503, "server_error", "brain_unavailable"),
            BackendErrorKind::NavigationFailed => {
                (502, "server_error", "browser_navigation_failed")
            }
            BackendErrorKind::SubmissionFailed => (502, "server_error", "brain_submission_failed"),
            BackendErrorKind::ResponseNotDetected => {
                (502, "server_error", "brain_response_not_detected")
            }
            BackendErrorKind::ExtractionFailed => (502, "server_error", "brain_extraction_failed"),
            BackendErrorKind::UnsupportedCapability => {
                (400, "invalid_request_error", "unsupported_capability")
            }
            BackendErrorKind::ReadinessUnknown => (503, "server_error", "brain_readiness_unknown"),
            BackendErrorKind::Internal => (500, "server_error", "internal_error"),
        };
        Self::new(status, error_type, error.message).with_code(code)
    }

    pub fn from_readiness(readiness: Readiness) -> Option<Self> {
        let error = match readiness {
            Readiness::Ready => return None,
            Readiness::LoginRequired => BackendError::new(
                BackendErrorKind::LoginRequired,
                "provider login is required",
            ),
            Readiness::Challenge => {
                BackendError::new(BackendErrorKind::Challenge, "provider challenge detected")
            }
            Readiness::RateLimited => BackendError::new(
                BackendErrorKind::RateLimited,
                "provider rate limit detected",
            ),
            Readiness::Unknown => BackendError::new(
                BackendErrorKind::ReadinessUnknown,
                "provider readiness could not be established",
            ),
            Readiness::Failed => BackendError::new(
                BackendErrorKind::BrowserUnavailable,
                "browser readiness probe failed",
            ),
        };
        Some(Self::from_backend(&error))
    }

    /// Serialize the standard error envelope without exposing implementation details.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"error":{{"message":"{}","type":"{}","param":{},"code":{}}}}}"#,
            escape_json(&self.message),
            escape_json(&self.error_type),
            optional_json_string(self.param.as_deref()),
            optional_json_string(self.code.as_deref())
        )
    }
}

fn optional_json_string(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", escape_json(value)),
        None => "null".to_owned(),
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            character if character <= '\u{1f}' => {
                const HEX: &[u8; 16] = b"0123456789abcdef";
                let byte = character as u8;
                escaped.push_str("\\u00");
                escaped.push(HEX[(byte >> 4) as usize] as char);
                escaped.push(HEX[(byte & 0x0f) as usize] as char);
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_openai_style_error_envelope() {
        let error = ApiError::new(400, "invalid_request_error", "Bad request");
        assert_eq!(
            error.to_json(),
            r#"{"error":{"message":"Bad request","type":"invalid_request_error","param":null,"code":null}}"#
        );
    }

    #[test]
    fn escapes_untrusted_strings_as_valid_json() {
        let error = ApiError::new(400, "invalid_request_error", "quote: \" line\nnext\u{0001}")
            .with_param("messages[0].content")
            .with_code("invalid_content");
        assert_eq!(
            error.to_json(),
            r#"{"error":{"message":"quote: \" line\nnext\u0001","type":"invalid_request_error","param":"messages[0].content","code":"invalid_content"}}"#
        );
    }

    #[test]
    fn backend_failures_map_to_stable_http_statuses_and_codes() {
        let cases = [
            (
                BackendErrorKind::LoginRequired,
                401,
                "provider_login_required",
            ),
            (
                BackendErrorKind::Challenge,
                503,
                "provider_challenge_required",
            ),
            (BackendErrorKind::RateLimited, 429, "provider_rate_limited"),
            (BackendErrorKind::Timeout, 504, "brain_timeout"),
            (BackendErrorKind::Cancelled, 408, "request_cancelled"),
            (
                BackendErrorKind::BrowserUnavailable,
                503,
                "brain_unavailable",
            ),
            (
                BackendErrorKind::NavigationFailed,
                502,
                "browser_navigation_failed",
            ),
            (
                BackendErrorKind::SubmissionFailed,
                502,
                "brain_submission_failed",
            ),
            (
                BackendErrorKind::ResponseNotDetected,
                502,
                "brain_response_not_detected",
            ),
            (
                BackendErrorKind::ExtractionFailed,
                502,
                "brain_extraction_failed",
            ),
            (
                BackendErrorKind::UnsupportedCapability,
                400,
                "unsupported_capability",
            ),
            (
                BackendErrorKind::ReadinessUnknown,
                503,
                "brain_readiness_unknown",
            ),
            (BackendErrorKind::Internal, 500, "internal_error"),
        ];
        for (kind, status, code) in cases {
            let error = BackendError::new(kind, "safe diagnostic");
            let api_error = ApiError::from_backend(&error);
            assert_eq!(api_error.status, status);
            assert_eq!(api_error.code.as_deref(), Some(code));
            assert!(api_error.to_json().contains("safe diagnostic"));
        }
    }

    #[test]
    fn readiness_states_are_not_assumed_ready() {
        assert!(ApiError::from_readiness(Readiness::Ready).is_none());
        assert_eq!(
            ApiError::from_readiness(Readiness::LoginRequired)
                .unwrap()
                .status,
            401
        );
        assert_eq!(
            ApiError::from_readiness(Readiness::Unknown).unwrap().status,
            503
        );
    }

    #[test]
    fn status_is_kept_separate_from_body() {
        let error = ApiError::new(401, "authentication_error", "Unauthorized");
        assert_eq!(error.status, 401);
        assert!(!error.to_json().contains("401"));
    }
}
