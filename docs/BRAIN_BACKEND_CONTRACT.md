# Project-owned BrainBackend contract

Status: P-020 design proposal, 2026-10-09. No implementation or runtime test is claimed.

## Decisions

- This contract is owned by this repository. It must not depend on WebAgent types, functions, or crate.
- The BrainBackend is not an HTTP handler: no OpenAI DTOs, routes, status codes, headers, or SSE framing cross this boundary.
- One backend call performs one text turn. No tool execution, shell actions, autonomous loops, attachments, image/audio, or API routing.
- Each Brain is owned by a dedicated worker thread because WebView objects may be thread-affine. Calls to the same Brain are serialized; different Brains may run concurrently.
- Cancellation is cooperative: disconnect requests cancellation, and the adapter checks it between bounded page operations. Do not claim it immediately stops remote generation.
- Each Brain uses an app-owned profile by default. Existing WebAgent profiles are never shared implicitly.

## Boundary

```text
API layer -> normalized request -> per-Brain worker -> BrainBackend
          -> browser driver/runtime -> web-chat UI
          <- normalized events and final result
```

The API layer owns protocol compatibility; BrainBackend only controls the browser and reports what it observes.

## Contract surface

The first Rust interface should expose:

- `start()`: launch the app-owned browser profile and page, returning a typed error on failure.
- `readiness()`: return an observable readiness state.
- `infer(request, event_sink)`: submit one normalized text prompt, emit optional assistant-text snapshots, and return the final response or a typed error.
- `shutdown()`: release owned browser resources and report cleanup failures.

Do not implement retries inside the backend. Deadlines, API errors and SSE serialization belong above it.

## Types and semantics

Readiness states:
- `Ready`: page appears ready for a turn.
- `LoginRequired`: login wall positively detected.
- `Challenge`: challenge wall positively detected.
- `RateLimited`: explicit quota/rate-limit state detected.
- `Unknown`: insufficient evidence; not equivalent to login-required.
- `Failed`: browser/page unavailable or probe failed.

Initial normalized request:
- `prompt: String`: text already normalized by the API layer.
- `deadline: Instant`: one absolute deadline across readiness, submission and waiting.
- `cancellation`: project-owned cloneable cancellation handle.

Events may contain full text snapshots, not deltas. A snapshot can be identical, appended, shortened or revised. The API layer decides how to encode changes; only claim incremental streaming after live tests demonstrate multiple changing snapshots. The final result is authoritative and successful only when the adapter has a defensible completion signal.

Typed errors must distinguish login-required, challenge, rate-limited, timeout, cancellation, browser-unavailable, navigation-failed, submission-failed, response-not-detected, extraction-failed, unsupported capability and internal failure. Errors must not include credentials, profile contents or full prompt/response text by default. HTTP status mapping is the API layer's job.

## Ownership and concurrency

- One worker owns one backend and all browser objects.
- Same-Brain turns serialize from readiness through final result.
- Different Brains have separate workers and profile ownership.
- Worker startup, request and shutdown have bounded waits and explicit failure states.
- An uncertain or failed-to-stop worker must not be silently reused.

## Out of scope

OpenAI DTOs/routes/auth/model IDs, SSE framing, tool protocols, attachments, media, Responses persistence, conversation restoration, pooling, profile cloning, auto-routing, provider retries and operator UI. Add them only after separate requirements and contract review.

## Verification gate

1. Compile-time proof of no WebAgent dependency/types and no OpenAI wire DTOs in the contract.
2. Fake-backend tests for readiness/errors, buffered text, multiple snapshots, cancellation and shutdown.
3. Manager tests for same-Brain serialization and independent Brain workers.
4. Test that readiness, submission and response waiting share one absolute deadline.
5. Adapter tests later use a disposable profile on the actual target OS. This design is not evidence of a successful build or live browser turn.

## Follow-up tasks

- P-021: Brain registry/model IDs.
- P-022: capability declarations.
- P-023: full normalized request/event types.
- P-024: conversation-state policy.
- P-025: configuration and secret handling.
- P-004: establish the Rust workspace before implementing the interface in code.

This is a contract proposal for review, not Rust implementation.
