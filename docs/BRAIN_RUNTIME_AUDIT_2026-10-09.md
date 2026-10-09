# Upstream audit — Brain and browser-runtime lifecycle

- **Audit date:** 2026-10-09
- **Upstream revision:** `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22`
- **Scope:** `BrainBackend`, `WebBrainBackend`, relay lifecycle, browser pool, page driver, WebView runtime and profile configuration.
- **Method:** source inspection of the audited revision. No live login/session or browser test was run; no profile data was read or copied.

## Conclusion

The runtime is reusable today **as part of the `webagent` library**, not as a small stand-alone folder. The existing abstraction has a clear interface, but the concrete implementation is coupled to configuration, selectors, authenticated browser profiles, shared browser pooling, profile cloning/write-back, WebView window/event-loop management, observer logic, timeout resolution, circuit breaking and diagnostic/benchmark events.

A narrow adapter in this repository can call the existing public browser-inference function without exposing those internals. If the project later requires an independent runtime crate, the correct work is a deliberate upstream extraction with dependency boundaries and lifecycle tests—not copying `brain.rs` and hoping the browser starts.

## Runtime flow traced in source

1. `api_bridge::inference::run_task_blocking/run_task_streaming` chooses a concrete Brain when the model ID is `webagent/auto`, computes an attachment-aware timeout, and obtains a process-wide mutex keyed by lowercase Brain ID.
2. It calls `browser_inference::complete_with_attachments` or `complete_streaming_with_attachments` with a normalized request.
3. `browser_inference` validates prompt, tool schemas and attachments; adds the provider identity and optional tool envelope; invokes `relay_single_turn_with_attachments_streaming`; normalizes provider UI text; then parses/validates any `WEBAGENT_INFERENCE/1` tool-call envelope.
4. `relay` checks the circuit breaker, constructs `WebBrainBackend::from_config(brain_id)`, resolves readiness and response timeouts, starts the backend, waits for readiness, and drives a fresh chat turn.
5. The relay calls `new_chat`, sends text/attachments, and waits for a response. Text-only turns may retry up to three times; attachment turns are deliberately limited to one attempt. Provider rate-limit/block states are terminal and update circuit-breaker/score state.
6. The relay calls `backend.stop()` at completion. In shared-pool mode this may release the backend's driver while keeping the shared tab alive according to pool/persistence policy; it is not equivalent to closing a fresh browser process on every request.
7. A successful turn returns normalized text or parsed tool calls to the API handler, which formats the requested API response. The calling harness—not this bridge—executes returned tools.

The per-Brain mutex is essential because a browser conversation UI is a mutable state machine, not a stateless inference endpoint. Different Brains may run concurrently; requests to the same Brain are serialized.

## Source-level lifecycle facts

### Brain configuration and authenticated state

- `WebBrainBackend::from_config` looks up the Brain in `config::brains()`, reads its URL and profile directory, and loads selectors via `config::load_selectors(brain_id)`.
- The browser does not log in on behalf of the API client. It relies on the user having authenticated the configured web-chat profile; readiness can return states such as `Ready`, `LoginRequired`, `Cloudflare`, `Unbestimmt` or `Error`.
- Profile configuration is more than a directory path. The profile code has special handling for Chromium/WebView2 cookies, Local Storage, Session Storage, IndexedDB, Network/Cookies and Local State. These are authentication/session artifacts.
- Therefore **browser profile directories are secrets**. Do not commit, upload to CI, include in Docker build context, attach to issues, or copy into fixtures. A clean test must use a disposable profile and explicitly state that live provider authentication is not part of unit tests.

### WebView and page driver

- `BrainBackend` defines start/stop/readiness, login, new-chat, send, response-wait/streaming, conversation restore and optional page access.
- `WebBrainBackend` implements the trait. With the `webview` feature it either uses the shared `BrowserPool` or launches a `WebViewRuntime` and opens a page. The non-WebView build returns a clear unavailable error; it is not a working headless inference runtime.
- `PageDriver` is the browser-control abstraction: evaluate JS, navigate, inspect URL, trusted input, click, keyboard and file-upload-related operations. `WebViewRuntime` runs a dedicated UI event-loop thread and dispatches synchronous calls through channels.
- The source comments call the hidden-window mode “headless”, but the implementation is an off-screen/hidden WebView window, not proof of a standalone browser engine with no desktop/platform dependencies.
- Navigation timeouts differ by path: an unpooled `WebBrainBackend::start` uses a 15-second navigation timeout; shared-pool startup/navigation uses 30 seconds in the inspected path. Inference/readiness/wait-response timeouts are resolved separately by the timeout module.
- The shared pool owns runtime/tab bookkeeping and resilient fallback behavior. It can fall back to an encapsulated profile/runtime after shared-browser failures; this brings in profile cloning and cleanup code.

### Operational behavior and recovery

- `relay` checks the circuit breaker before work; terminal rate-limit or blocked states are recorded and not retried as if they were ordinary transient failures.
- Empty/UI-chrome-only responses are filtered. Provider errors are distinguished from actual answers, and the API layer can return a provider error rather than treating a banner as successful content.
- Text turns may retry with a fresh chat after send/wait failures. A turn may have been submitted even if its response was not detected, so the retry can leave an abandoned conversation; this is documented in the relay comments.
- Browser and API failures should be kept distinct: malformed API request (4xx), unavailable/login-required Brain (provider/runtime error), deadline exceeded, attachment capability unsupported, and internal browser/runtime failure. Do not flatten all of these into a generic empty completion.
- Shared runtime state means graceful shutdown and profile write-back semantics need explicit tests; dropping a per-request backend is not necessarily equivalent to tearing down the pooled WebView.

## Runtime dependency map

| Component | Role | Important dependencies | Stand-alone extraction assessment |
|---|---|---|---|
| `brain.rs` | Backend trait and session/response types | Shared vocabulary only | Good conceptual interface; insufficient by itself |
| `browser/mod.rs` + `browser/backend.rs` | Concrete Brain implementation, selectors, UI and response detection | `config`, `page_driver`, `webview_runtime`, `browser_pool`, `webview_reveal`, `observer`, `protocol` | Too coupled to copy alone |
| `browser_inference.rs` | Normalized one-turn inference, tool envelope, attachment validation, text streaming | `relay`, `observer`, `bench_events` | Best adapter seam, but currently imports upstream-global concerns |
| `relay.rs` | Start/readiness/new-chat/send/wait/retry/cleanup | `brain`, `browser`, `timeouts`, `circuit_breaker`, `brain_score`, `capability_proof`, `bench_events`, `observer` | Reuse through the upstream library; porting requires dependency reduction |
| `browser_pool.rs` | Shared WebView and tabs, resilient fallback, profile isolation | `config`, `browser`, `webview_runtime`, `brain_grid`, `webview_reveal`, events | Runtime subsystem, not a small adapter |
| `webview_runtime.rs` | UI thread, WebView creation, JS/CDP-style in-process operations | `wry`, `tao`, `page_driver`, platform helpers and upload scripts | Large, platform-sensitive core |
| `page_driver.rs` | Common page-control trait and errors | `serde_json`, runtime implementations | Useful interface to preserve |
| `config/brains.rs`, `config/profiles.rs`, related config modules | Brain registry, selectors, profile directories/cloning/write-back | Paths, environment, filesystem, benchmark diagnostics, worker constants | Needs an explicit new config contract if extracted |

## Reuse implications

### Recommended now: reuse the upstream library boundary

Use the public `webagent::browser_inference`/API bridge boundary via a pinned dependency for the prototype. It retains the tested browser profile/session behavior and avoids reimplementing provider-specific DOM selectors, file uploads, login detection, WebView lifetime, profile locks and recovery. The wrapper should own only this project's configuration, process lifecycle, security boundary, health reporting and UI/test console.

### Not recommended now: extract only `BrainBackend`

The trait is not a process-independent adapter. A new implementation would still need a compatible Brain registry, selectors, session profile lifecycle, WebView runtime, page driver and cleanup. Pretending otherwise would move the complexity into hidden glue code.

### Long-term extraction requirements

If independence from `webagent-rs` becomes a requirement, create a dedicated runtime interface with:
- explicit `start/ready/infer/stream/cancel/shutdown` lifecycle;
- no dependency on CLI/TUI, agent controller, shell executor, benchmark scoring or autonomous worker systems;
- injected profile/selector/config roots;
- per-Brain concurrency policy and cancellation semantics;
- redacted diagnostics and disposable-profile tests;
- OS-specific integration tests for Windows WebView2 and supported Linux WebView environments;
- a migration plan for browser-profile persistence and compatibility changes.

Do not make a browser runtime library public until its dependency graph and lifecycle guarantees are testable without the full application.

## Verification status

- **Verified by source inspection:** the lifecycle described above is present in the audited revision; the `webview` feature gates real runtime startup; shared pool/profile state is part of the implementation.
- **Not verified in this audit:** clean checkout compilation, current CI result, live login, live text streaming, attachment upload, concurrent requests, graceful shutdown, Windows/Linux packaging.
- **No secrets or profile data were accessed or copied.**
