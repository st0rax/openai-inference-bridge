# Initial HTTP Server

The initial listener is a bounded, synchronous HTTP/1.0 and HTTP/1.1 prototype implemented without external crates. It is intentionally narrow and is not yet an OpenAI-compatible inference service.

## Startup

The executable loads configuration from the environment, then binds to the configured loopback address (default `127.0.0.1:8788`). A valid `OIB_API_TOKEN` is currently required by configuration loading, but request authentication is a separate task and is not yet enforced by this listener.

For safety, the runtime rejects every non-loopback bind address even if `OIB_ALLOW_REMOTE=1`. This restriction remains until the authentication layer is implemented and verified.

## Current route table

| Request | Result |
| --- | --- |
| `GET /v1/models` | `501 Not Implemented` |
| `POST /v1/chat/completions` | `501 Not Implemented` |
| Known route with wrong method | `405 Method Not Allowed`, with `Allow` header |
| Unknown route | `404 Not Found` |
| Malformed request headers/line | `400 Bad Request` |
| Request header timeout | `408 Request Timeout` |

Responses use JSON error bodies, include a byte-accurate `Content-Length`, and close the connection. No request body is parsed or dispatched. The listener does not yet authenticate requests, parse OpenAI payloads, or invoke a Brain.

## Defensive limits and known limitations

- Request headers are capped at 16 KiB.
- Reading request headers has a five-second timeout.
- Only origin-form request targets and HTTP/1.0 or HTTP/1.1 are accepted.
- The listener handles connections sequentially. This is a bootstrap implementation, not a throughput-ready production server.
- It does not support keep-alive, chunked request bodies, TLS, CORS, or inference endpoints.
- Do not expose it to a network. Remote binding is explicitly refused by the runtime.

The next tasks add authentication, structured errors, model discovery, and Chat Completions. Keep the current `501` responses until each endpoint is actually implemented and tested; do not advertise a route as working merely because it appears in the route table.
