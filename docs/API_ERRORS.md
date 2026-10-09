# API Error Policy

All current HTTP failures use the OpenAI-style envelope:

```json
{
  "error": {
    "message": "Human-readable message",
    "type": "invalid_request_error",
    "param": null,
    "code": null
  }
}
```

The HTTP status is carried by the response status line, not duplicated in the JSON body. The `ApiError` type serializes `message`, `type`, `param`, and `code`; absent optional fields are `null`. Message and optional strings are JSON-escaped, including quotes, backslashes, newlines, and control characters.

## Current mappings

| HTTP status | Error type | Current use |
| --- | --- | --- |
| 400 | `invalid_request_error` | Malformed HTTP request |
| 401 | `authentication_error` | Missing or invalid bearer token |
| 404 | `not_found_error` | Unknown route |
| 405 | `invalid_request_error` | Known route with wrong method |
| 408 | `timeout_error` | Request headers not received before timeout |
| 501 | `not_implemented_error` | Placeholder inference routes |

The authentication response includes `WWW-Authenticate: Bearer`; method errors include `Allow`. Responses are marked `Cache-Control: no-store`.

## Error handling rules

- Keep client-facing messages generic and safe. Do not include tokens, Authorization values, browser profile paths, provider cookies, prompts, or internal exception details.
- Keep HTTP status selection in the HTTP/API layer; Brain adapters return typed internal failures rather than HTTP responses.
- Use stable error types and codes when request validation and Brain dispatch are added.
- Do not claim OpenAI compatibility for error cases that have not been tested against representative clients.

The current route table still returns `501` for `/v1/models` and `/v1/chat/completions`. This document defines error envelopes; it does not claim those endpoints are implemented.
