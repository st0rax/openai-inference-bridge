//! Minimal HTTP/1.x listener and route table.
//!
//! This initial server intentionally serves no inference data yet. Functional API
//! endpoints are added by later tasks. Remote binding remains disabled until
//! request authentication is implemented.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    method: String,
    target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Response {
    status: u16,
    reason: &'static str,
    body: &'static str,
    allow: Option<&'static str>,
}

impl Response {
    fn new(status: u16, reason: &'static str, body: &'static str) -> Self {
        Self {
            status,
            reason,
            body,
            allow: None,
        }
    }
}

fn route(request: &Request) -> Response {
    match (request.method.as_str(), request.target.as_str()) {
        ("GET", "/v1/models") => Response::new(
            501,
            "Not Implemented",
            r#"{"error":{"type":"not_implemented","message":"GET /v1/models is not implemented yet"}}"#,
        ),
        ("POST", "/v1/chat/completions") => Response::new(
            501,
            "Not Implemented",
            r#"{"error":{"type":"not_implemented","message":"POST /v1/chat/completions is not implemented yet"}}"#,
        ),
        (_, "/v1/models") => Response {
            allow: Some("GET"),
            ..Response::new(
                405,
                "Method Not Allowed",
                r#"{"error":{"type":"method_not_allowed","message":"This method is not supported for /v1/models"}}"#,
            )
        },
        (_, "/v1/chat/completions") => Response {
            allow: Some("POST"),
            ..Response::new(
                405,
                "Method Not Allowed",
                r#"{"error":{"type":"method_not_allowed","message":"This method is not supported for /v1/chat/completions"}}"#,
            )
        },
        _ => Response::new(
            404,
            "Not Found",
            r#"{"error":{"type":"not_found","message":"Route not found"}}"#,
        ),
    }
}

/// Start the initial local HTTP listener.
///
/// Non-loopback addresses are rejected here even if configuration opt-in is set:
/// this server does not yet authenticate requests.
pub fn run(bind_addr: SocketAddr) -> io::Result<()> {
    if !bind_addr.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "remote binding is disabled until HTTP authentication is implemented",
        ));
    }

    let listener = TcpListener::bind(bind_addr)?;
    eprintln!("HTTP listener bound to {}", listener.local_addr()?);

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(error) = stream.set_read_timeout(Some(READ_TIMEOUT)) {
                    eprintln!("failed to set HTTP read timeout: {error}");
                    continue;
                }
                if let Err(error) = handle_connection(&mut stream) {
                    eprintln!("HTTP connection ended: {error}");
                }
            }
            Err(error) => eprintln!("HTTP accept failed: {error}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream) -> io::Result<()> {
    let response = match read_request(stream) {
        Ok(request) => route(&request),
        Err(error)
            if error.kind() == io::ErrorKind::TimedOut
                || error.kind() == io::ErrorKind::WouldBlock =>
        {
            Response::new(
                408,
                "Request Timeout",
                r#"{"error":{"type":"request_timeout","message":"Timed out reading request headers"}}"#,
            )
        }
        Err(_) => Response::new(
            400,
            "Bad Request",
            r#"{"error":{"type":"bad_request","message":"Malformed HTTP request"}}"#,
        ),
    };
    write_response(stream, &response)
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
    let request_line = head
        .split("\r\n")
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

    Ok(Request {
        method: method.expect("checked above").to_owned(),
        target: target.to_owned(),
    })
}

fn write_response(stream: &mut impl Write, response: &Response) -> io::Result<()> {
    let mut headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.reason,
        response.body.len()
    );
    if let Some(allow) = response.allow {
        headers.push_str(&format!("Allow: {allow}\r\n"));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes())?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn known_routes_are_explicitly_not_implemented_yet() {
        let models = route(&Request {
            method: "GET".into(),
            target: "/v1/models".into(),
        });
        assert_eq!(models.status, 501);

        let completions = route(&Request {
            method: "POST".into(),
            target: "/v1/chat/completions".into(),
        });
        assert_eq!(completions.status, 501);
    }

    #[test]
    fn known_route_rejects_wrong_method() {
        let response = route(&Request {
            method: "POST".into(),
            target: "/v1/models".into(),
        });
        assert_eq!(response.status, 405);
        assert_eq!(response.allow, Some("GET"));
    }

    #[test]
    fn unknown_route_is_not_found() {
        let response = route(&Request {
            method: "GET".into(),
            target: "/private".into(),
        });
        assert_eq!(response.status, 404);
    }

    #[test]
    fn parses_simple_http_request() {
        let bytes = b"GET /v1/models HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let request = read_request(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.target, "/v1/models");
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
        let error = run(address).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
