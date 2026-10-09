# Readiness and Failure Mapping

The runtime contract exposes explicit readiness states and typed backend failures. `ApiError::from_readiness` and `ApiError::from_backend` map these to stable HTTP statuses and error codes; the mapping is unit-tested.

| Condition | HTTP | Error type | Code |
| --- | ---: | --- | --- |
| Login required | 401 | `authentication_error` | `provider_login_required` |
| Provider challenge | 503 | `server_error` | `provider_challenge_required` |
| Rate limited | 429 | `rate_limit_error` | `provider_rate_limited` |
| Deadline expired | 504 | `timeout_error` | `brain_timeout` |
| Request cancelled | 408 | `timeout_error` | `request_cancelled` |
| Browser unavailable | 503 | `server_error` | `brain_unavailable` |
| Navigation or submission failure | 502 | `server_error` | `browser_navigation_failed` / `brain_submission_failed` |
| Completion not detected or extraction failed | 502 | `server_error` | `brain_response_not_detected` / `brain_extraction_failed` |
| Unsupported capability | 400 | `invalid_request_error` | `unsupported_capability` |
| Unknown readiness | 503 | `server_error` | `brain_readiness_unknown` |
| Internal failure | 500 | `server_error` | `internal_error` |

## Operational rules

- Never treat unknown readiness as ready.
- Do not retry a turn automatically after a timeout, cancellation, submission failure, response-detection failure, or extraction failure. The worker manager poisons uncertain workers.
- Login/challenge/rate-limit states remain distinguishable to callers; do not hide them behind a generic success or silently start another session.
- Error messages must remain safe diagnostics. Never include prompts, reply text, Authorization values, cookies, or profile paths.
- A `Ready` status means only that the concrete driver has positively identified a usable provider page. It is not inferred from a successful navigation alone.

**Current limitation:** mapping is implemented and tested, but no concrete WebView driver is connected to the executable. The live provider readiness states are therefore not yet observed in production code, and Chat Completions still returns `503 brain_runtime_unavailable` for configured models.
