//! HTTP/1.x listener and OpenAI-compatible route table.
//!
//! The listener is loopback-only; remote transport security is not implemented.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    api_error::ApiError,
    brain_backend::{
        BackendError, BrainBackend, BrowserBrainBackend, CancellationToken, InferenceRequest,
        Readiness,
    },
    brain_manager::BrainManager,
    brain_registry::BrainRegistry,
    browser_driver::BrowserCdpDriver,
    chat_completion::{completion_response_json, normalize_request},
    config::Config,
};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const HTTP_WORKERS: usize = 4;
const MAX_PENDING_CONNECTIONS: usize = 32;
const BRAIN_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    method: String,
    target: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Response {
    status: u16,
    reason: &'static str,
    body: String,
    allow: Option<&'static str>,
    www_authenticate: Option<&'static str>,
}

impl Response {
    fn json(status: u16, reason: &'static str, body: String) -> Self {
        Self {
            status,
            reason,
            body,
            allow: None,
            www_authenticate: None,
        }
    }

    fn api_error(reason: &'static str, error: ApiError) -> Self {
        Self {
            status: error.status,
            reason,
            body: error.to_json(),
            allow: None,
            www_authenticate: None,
        }
    }

    fn error(
        status: u16,
        reason: &'static str,
        error_type: &'static str,
        message: &'static str,
    ) -> Self {
        Self::api_error(reason, ApiError::new(status, error_type, message))
    }
}

fn route(request: &Request, registry: &BrainRegistry, manager: &BrainManager) -> Response {
    match (request.method.as_str(), request.target.as_str()) {
        ("GET", "/v1/models") => Response::json(200, "OK", registry.models_json()),
        ("POST", "/v1/chat/completions") => {
            let is_json = request
                .content_type
                .as_deref()
                .and_then(|value| value.split(';').next())
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
            if !is_json {
                return Response::api_error(
                    "Unsupported Media Type",
                    ApiError::new(
                        415,
                        "invalid_request_error",
                        "Content-Type must be application/json",
                    )
                    .with_param("Content-Type")
                    .with_code("unsupported_media_type"),
                );
            }

            let normalized = match normalize_request(&request.body) {
                Ok(request) => request,
                Err(error) => {
                    let reason = if error.status == 413 {
                        "Payload Too Large"
                    } else {
                        "Bad Request"
                    };
                    return Response::api_error(reason, error);
                }
            };

            if registry.resolve_model_id(&normalized.model_id).is_none() {
                return Response::api_error(
                    "Not Found",
                    ApiError::new(
                        404,
                        "model_not_found",
                        "The requested model is not configured or enabled",
                    )
                    .with_param("model")
                    .with_code("model_not_found"),
                );
            }

            let brain_id = registry
                .resolve_model_id(&normalized.model_id)
                .expect("model was resolved above")
                .id
                .clone();
            let model_id = normalized.model_id.clone();
            let deadline = Instant::now() + BRAIN_REQUEST_TIMEOUT;

            if let Err(error) = manager.start(&brain_id, deadline) {
                let api_error = ApiError::from_backend(&error);
                return Response::api_error(reason_for_status(api_error.status), api_error);
            }

            match manager.readiness(&brain_id, deadline) {
                Ok(readiness) => {
                    if let Some(api_error) = ApiError::from_readiness(readiness) {
                        return Response::api_error(reason_for_status(api_error.status), api_error);
                    }
                }
                Err(error) => {
                    let api_error = ApiError::from_backend(&error);
                    return Response::api_error(reason_for_status(api_error.status), api_error);
                }
            }

            let inference = InferenceRequest {
                prompt: normalized.compose_prompt(),
                deadline,
                cancellation: CancellationToken::default(),
            };
            let content = match manager.infer(&brain_id, inference, &mut |_| {}) {
                Ok(content) => content,
                Err(error) => {
                    let api_error = ApiError::from_backend(&error);
                    return Response::api_error(reason_for_status(api_error.status), api_error);
                }
            };
            Response::json(200, "OK", completion_response_json(&model_id, &content))
        }
        (_, "/v1/models") => Response {
            allow: Some("GET"),
            ..Response::error(
                405,
                "Method Not Allowed",
                "invalid_request_error",
                "This method is not supported for /v1/models",
            )
        },
        (_, "/v1/chat/completions") => Response {
            allow: Some("POST"),
            ..Response::error(
                405,
                "Method Not Allowed",
                "invalid_request_error",
                "This method is not supported for /v1/chat/completions",
            )
        },
        _ => Response::error(404, "Not Found", "not_found_error", "Route not found"),
    }
}

/// Start the local HTTP listener. Non-loopback binding remains forbidden even
/// when configuration opted in, because TLS and remote deployment hardening are absent.
pub fn run(config: Config) -> io::Result<()> {
    validate_bind(config.bind_addr)?;
    let registry = Arc::new(
        BrainRegistry::from_env(&config)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
    );
    let manager = Arc::new(BrainManager::new((*registry).clone(), |brain| {
        let backend = BrowserBrainBackend::new(brain, BrowserCdpDriver::new())?;
        Ok(Box::new(backend) as Box<dyn BrainBackend>)
    }));
    let token = Arc::new(config.api_token().to_owned());
    let listener = TcpListener::bind(config.bind_addr)?;
    eprintln!("HTTP listener bound to {}", listener.local_addr()?);

    let (sender, receiver) = mpsc::sync_channel::<TcpStream>(MAX_PENDING_CONNECTIONS);
    let receiver = Arc::new(Mutex::new(receiver));
    for worker_index in 0..HTTP_WORKERS {
        let worker_receiver = Arc::clone(&receiver);
        let worker_token = Arc::clone(&token);
        let worker_registry = Arc::clone(&registry);
        let worker_manager = Arc::clone(&manager);
        thread::Builder::new()
            .name(format!("oib-http-{worker_index}"))
            .spawn(move || loop {
                let incoming = match worker_receiver.lock() {
                    Ok(receiver) => receiver.recv(),
                    Err(_) => return,
                };
                let Ok(mut stream) = incoming else {
                    break;
                };
                if stream.set_read_timeout(Some(READ_TIMEOUT)).is_err()
                    || stream.set_write_timeout(Some(WRITE_TIMEOUT)).is_err()
                {
                    continue;
                }
                if let Err(error) = handle_connection(
                    &mut stream,
                    &worker_token,
                    &worker_registry,
                    &worker_manager,
                ) {
                    eprintln!("HTTP connection ended: {error}");
                }
            })
            .map_err(io::Error::other)?;
    }

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                if sender.send(stream).is_err() {
                    return Err(io::Error::other("HTTP worker pool stopped"));
                }
            }
            Err(error) => eprintln!("HTTP accept failed: {error}"),
        }
    }

    Ok(())
}

fn validate_bind(bind_addr: SocketAddr) -> io::Result<()> {
    if !bind_addr.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "remote binding is disabled until transport security is implemented",
        ));
    }
    Ok(())
}

fn handle_connection(
    stream: &mut TcpStream,
    expected_token: &str,
    registry: &BrainRegistry,
    manager: &BrainManager,
) -> io::Result<()> {
    let response = match read_request(stream) {
        Ok(request) => dispatch(&request, expected_token, registry, manager),
        Err(error)
            if error.kind() == io::ErrorKind::TimedOut
                || error.kind() == io::ErrorKind::WouldBlock =>
        {
            Response::error(
                408,
                "Request Timeout",
                "timeout_error",
                "Timed out reading request",
            )
        }
        Err(error) if error.kind() == io::ErrorKind::FileTooLarge => Response::error(
            413,
            "Payload Too Large",
            "invalid_request_error",
            "Request body exceeds the 1 MiB limit",
        ),
        Err(_) => Response::error(
            400,
            "Bad Request",
            "invalid_request_error",
            "Malformed HTTP request",
        ),
    };
    write_response(stream, &response)
}

fn dispatch(request: &Request, expected_token: &str, registry: &BrainRegistry, manager: &BrainManager) -> Response {
    if !is_authorized(request.authorization.as_deref(), expected_token) {
        return Response {
            www_authenticate: Some("Bearer"),
            ..Response::error(
                401,
                "Unauthorized",
                "authentication_error",
                "Missing or invalid bearer token",
            )
        };
    }
    route(request, registry, manager)
}

fn reason_for_status(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Internal Server Error",
    }
}

fn is_authorized(header: Option<&str>, expected_token: &str) -> bool {
    let Some(header) = header else {
        return false;
    };
    let mut parts = header.split_whitespace();
    let (Some(scheme), Some(candidate)) = (parts.next(), parts.next()) else {
        return false;
    };
    if parts.next().is_some() || !scheme.eq_ignore_ascii_case("Bearer") {
        return false;
    }
    constant_time_eq(candidate.as_bytes(), expected_token.as_bytes())
}

/// Compare all bytes up to the longer input length without early exit on a mismatch.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for (left_byte, right_byte) in left
        .iter()
        .copied()
        .chain(std::iter::repeat(0))
        .zip(right.iter().copied().chain(std::iter::repeat(0)))
        .take(left.len().max(right.len()))
    {
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}

fn read_request(reader: &mut impl Read) -> io::Result<Request> {
    let mut head = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];

    loop {
        if head.len() >= MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP request headers exceed the size limit",
            ));
        }

        match reader.read(&mut byte)? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed before request headers completed",
                ));
            }
            _ => head.push(byte[0]),
        }

        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    let head = std::str::from_utf8(&head)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "request headers are not UTF-8"))?;
    let without_terminator = &head[..head.len() - 4];
    let mut lines = without_terminator.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next();
    let target = parts.next();
    let version = parts.next();

    if parts.next().is_some()
        || method.is_none()
        || target.is_none()
        || !matches!(version, Some("HTTP/1.0" | "HTTP/1.1"))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid HTTP request line",
        ));
    }

    let target = target.expect("checked above");
    if !target.starts_with('/') || target.starts_with("//") || target.contains('#') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only origin-form request targets are supported",
        ));
    }

    let mut authorization = None;
    let mut content_type = None;
    let mut content_length = None;
    let mut has_transfer_encoding = false;
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "obsolete folded headers are not supported",
            ));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed HTTP header"))?;
        if name.is_empty() || !name.bytes().all(is_header_name_char) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP header name",
            ));
        }
        if value
            .bytes()
            .any(|byte| (byte < 0x20 && byte != b'\t') || byte == 0x7f)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid control character in HTTP header",
            ));
        }
        let value = value.trim_matches(|character| character == ' ' || character == '\t');
        if name.eq_ignore_ascii_case("authorization") {
            if authorization.is_some() || value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Authorization header",
                ));
            }
            authorization = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-type") {
            if content_type.is_some() || value.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Content-Type header",
                ));
            }
            content_type = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some()
                || value.is_empty()
                || !value.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid Content-Length header",
                ));
            }
            let length = value.parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length header")
            })?;
            if length > MAX_BODY_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "request body exceeds the size limit",
                ));
            }
            content_length = Some(length);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            has_transfer_encoding = true;
        }
    }

    if has_transfer_encoding {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Transfer-Encoding is not supported",
        ));
    }

    let mut body = vec![0_u8; content_length.unwrap_or(0)];
    reader.read_exact(&mut body)?;

    Ok(Request {
        method: method.expect("checked above").to_owned(),
        target: target.to_owned(),
        authorization,
        content_type,
        body,
    })
}

fn is_header_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn write_response(stream: &mut impl Write, response: &Response) -> io::Result<()> {
    let mut headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n",
        response.status,
        response.reason,
        response.body.len()
    );
    if let Some(allow) = response.allow {
        headers.push_str(&format!("Allow: {allow}\r\n"));
    }
    if let Some(challenge) = response.www_authenticate {
        headers.push_str(&format!("WWW-Authenticate: {challenge}\r\n"));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes())?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::io::Cursor;

    const TOKEN: &str = "test-token-that-is-at-least-32-characters";

    fn request(method: &str, target: &str) -> Request {
        Request {
            method: method.to_owned(),
            target: target.to_owned(),
            authorization: None,
            content_type: None,
            body: Vec::new(),
        }
    }

    fn chat_request(body: &[u8]) -> Request {
        let mut request = request("POST", "/v1/chat/completions");
        request.content_type = Some("application/json".to_owned());
        request.body = body.to_vec();
        request
    }

    fn configured_registry() -> BrainRegistry {
        let data_dir = env::temp_dir().to_string_lossy().into_owned();
        let config = Config::from_lookup(|key| match key {
            "OIB_API_TOKEN" => Some(TOKEN.to_owned()),
            "OIB_DATA_DIR" => Some(data_dir.clone()),
            _ => None,
        })
        .unwrap();
        BrainRegistry::from_lookup(&config, |key| match key {
            "OIB_BRAIN_IDS" => Some("chatgpt".to_owned()),
            "OIB_BRAIN_CHATGPT_ENABLED" => Some("1".to_owned()),
            "OIB_BRAIN_CHATGPT_LABEL" => Some("ChatGPT".to_owned()),
            "OIB_BRAIN_CHATGPT_URL" => Some("https://chatgpt.com/".to_owned()),
            "OIB_BRAIN_CHATGPT_ADAPTER" => Some("chatgpt-web".to_owned()),
            _ => None,
        })
        .unwrap()
    }

    struct FakeBackend;

    impl BrainBackend for FakeBackend {
        fn start(&mut self, _deadline: Instant) -> Result<(), BackendError> {
            Ok(())
        }

        fn readiness(&mut self, _deadline: Instant) -> Result<Readiness, BackendError> {
            Ok(Readiness::Ready)
        }

        fn infer(
            &mut self,
            request: &InferenceRequest,
            event_sink: &mut dyn FnMut(crate::brain_backend::BackendEvent),
        ) -> Result<String, BackendError> {
            event_sink(crate::brain_backend::BackendEvent::AssistantTextSnapshot(
                "fake reply".to_owned(),
            ));
            Ok(format!("fake reply for {}", request.prompt))
        }

        fn shutdown(&mut self, _deadline: Instant) -> Result<(), BackendError> {
            Ok(())
        }
    }

    fn test_manager(registry: BrainRegistry) -> BrainManager {
        BrainManager::new(registry, |_| {
            Ok(Box::new(FakeBackend) as Box<dyn BrainBackend>)
        })
    }

    #[test]
    fn model_list_and_chat_completion_route_use_registry_and_backend() {
        let empty_registry = BrainRegistry::empty();
        let empty_manager = test_manager(empty_registry.clone());
        let models = route(
            &request("GET", "/v1/models"),
            &empty_registry,
            &empty_manager,
        );
        assert_eq!(models.status, 200);
        assert_eq!(models.body, r#"{"object":"list","data":[]}"#);

        let missing_content_type = route(
            &request("POST", "/v1/chat/completions"),
            &empty_registry,
            &empty_manager,
        );
        assert_eq!(missing_content_type.status, 415);

        let body = br#"{"model":"oib/chatgpt","messages":[{"role":"user","content":"hi"}]}"#;
        let missing_model = route(&chat_request(body), &empty_registry, &empty_manager);
        assert_eq!(missing_model.status, 404);
        assert!(missing_model.body.contains("model_not_found"));

        let registry = configured_registry();
        let manager = test_manager(registry.clone());
        let response = route(&chat_request(body), &registry, &manager);
        assert_eq!(response.status, 200);
        assert!(response.body.contains(r#""object":"chat.completion""#));
        assert!(response.body.contains("fake reply for"));
    }

    #[test]
    fn known_route_rejects_wrong_method() {
        let response = route(&request("POST", "/v1/models"), &BrainRegistry::empty(), &test_manager(BrainRegistry::empty()));
        assert_eq!(response.status, 405);
        assert_eq!(response.allow, Some("GET"));
    }

    #[test]
    fn unknown_route_is_not_found() {
        assert_eq!(
            route(&request("GET", "/private"), &BrainRegistry::empty(), &test_manager(BrainRegistry::empty())).status,
            404
        );
    }

    #[test]
    fn bearer_authentication_is_required_and_case_insensitive_for_scheme() {
        assert!(!is_authorized(None, TOKEN));
        assert!(!is_authorized(Some("Bearer wrong-token"), TOKEN));
        assert!(!is_authorized(Some("Basic abc"), TOKEN));
        assert!(!is_authorized(Some("Bearer token extra"), TOKEN));
        let valid_header = format!("Bearer {TOKEN}");
        let lowercase_header = format!("bearer {TOKEN}");
        assert!(is_authorized(Some(&valid_header), TOKEN));
        assert!(is_authorized(Some(&lowercase_header), TOKEN));
    }

    #[test]
    fn authentication_runs_before_route_dispatch() {
        let request = request("GET", "/v1/models");
        let empty_registry = BrainRegistry::empty();
        let manager = test_manager(empty_registry.clone());
        let unauthorized = dispatch(&request, TOKEN, &empty_registry, &manager);
        assert_eq!(unauthorized.status, 401);
        assert_eq!(unauthorized.www_authenticate, Some("Bearer"));
        assert!(
            unauthorized
                .body
                .contains("\"type\":\"authentication_error\"")
        );
        assert!(unauthorized.body.contains("\"param\":null,\"code\":null"));

        let mut authorized_request = request;
        authorized_request.authorization = Some(format!("Bearer {TOKEN}"));
        assert_eq!(
            dispatch(&authorized_request, TOKEN, &empty_registry, &manager).status,
            200
        );
    }

    #[test]
    fn constant_time_comparison_checks_length_and_content() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"samf"));
        assert!(!constant_time_eq(b"same", b"same-longer"));
    }

    #[test]
    fn parses_authorization_header_without_echoing_it() {
        let bytes = format!(
            "GET /v1/models HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\n\r\n"
        );
        let request = read_request(&mut Cursor::new(bytes)).unwrap();
        let expected = format!("Bearer {TOKEN}");
        assert_eq!(request.authorization.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn reads_content_length_and_json_body() {
        let body = r#"{"model":"oib/chatgpt","messages":[{"role":"user","content":"hi"}]}"#;
        let request_text = format!(
            "POST /v1/chat/completions HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let request = read_request(&mut Cursor::new(request_text)).unwrap();
        assert_eq!(request.content_type.as_deref(), Some("application/json"));
        assert_eq!(request.body, body.as_bytes());
    }

    #[test]
    fn rejects_oversized_request_body_before_reading_it() {
        let request_text = format!(
            "POST /v1/chat/completions HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        let error = read_request(&mut Cursor::new(request_text)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::FileTooLarge);
    }

    #[test]
    fn rejects_duplicate_authorization_headers() {
        let bytes = b"GET /v1/models HTTP/1.1\r\nAuthorization: Bearer first\r\nAuthorization: Bearer second\r\n\r\n";
        assert!(read_request(&mut Cursor::new(bytes)).is_err());
    }

    #[test]
    fn rejects_absolute_request_target_and_invalid_version() {
        let absolute = b"GET http://example.com/v1/models HTTP/1.1\r\nHost: example.com\r\n\r\n";
        assert!(read_request(&mut Cursor::new(absolute)).is_err());

        let version = b"GET /v1/models HTTP/2\r\nHost: localhost\r\n\r\n";
        assert!(read_request(&mut Cursor::new(version)).is_err());
    }

    #[test]
    fn rejects_oversized_headers() {
        let bytes = vec![b'a'; MAX_HEADER_BYTES + 1];
        let error = read_request(&mut Cursor::new(bytes)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn refuses_remote_binding_before_opening_listener() {
        let address = "0.0.0.0:8788".parse().unwrap();
        let error = validate_bind(address).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
