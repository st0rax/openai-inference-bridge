# WebAgent component evaluation — initial browser-text path

**Date:** 2026-10-09  
**Upstream:** `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22`, package `webagent 0.11.3`.  
**Scope:** launch an isolated browser profile, open a web-chat, submit one text prompt, detect the assistant reply, and return buffered text or incremental snapshots.

## Binding result

**No WebAgent source is approved for direct copying/adaptation by this evaluation.** The product owns its OpenAI-compatible API and Brain contract. Use this upstream only as behavioral evidence. Any later proposal to port a specific function requires a narrower record naming exact symbol/lines, rationale, changed implementation and tests. No WebAgent crate dependency is approved.

## 1. Brain contract — `src/brain.rs`

- **Source:** `SessionState` / `BrainResponse`, lines 3–58; `BrainBackend`, lines 60–134. SHA `bdce11f16d7cdf56b85b866559b261c5f3f61dff`.
- **Purpose:** lifecycle, readiness/login state, prompt submission, waiting and streaming.
- **Dependencies/limitations:** trait is fairly isolated but synchronous and includes UI reveal/park, login-click, conversation restore, `Any` page exposure and upstream-specific response telemetry. Default streaming emits only a final snapshot, so it does not promise incremental streaming. Dummy tests exist in source but were not run here.
- **Security/license:** upstream declares MIT; copied code would require retained notices. Transitive license closure is not audited.
- **Alternatives:** project-owned contract with explicit deadline, cancellation, readiness/failure categories, text response events and cleanup.
- **Decision: REIMPLEMENT.** Preserve only the lifecycle concepts; do not copy the trait or its types.
- **Tests:** fake backend; Ready/LoginRequired/Challenge/Unknown/Error; buffered and incremental turn; timeout and shutdown; no WebAgent types in public/internal project contract.

## 2. Page driver — `src/page_driver.rs`

- **Source:** error type lines 9–29; trait lines 32–156. SHA `65688f0a26151fd231986fd1c5d1f197487feb6f`.
- **Purpose:** isolate page control from provider-specific logic.
- **Dependencies/limitations:** mainly `std` and `serde_json`, but the interface has accumulated uploads, OS clipboard, trusted input, pointer and screenshot methods irrelevant to the first text-only slice. Several methods default to `NotAvailable`; trait presence does not mean a concrete driver supports them. Existing tests cover selector fallback, not real browser correctness.
- **Security/license:** arbitrary JS evaluation must stay internal and never be exposed through HTTP. MIT applies to copied code.
- **Decision: REIMPLEMENT SMALLER.** Start with navigation, bounded page evaluation if needed, current URL, trusted text entry/submit and explicit errors. Add uploads/screenshots only after a requirement.
- **Tests:** mock contract, closed driver, timeouts, live submission/extraction on target OS.

## 3. Embedded runtime — `src/webview_runtime.rs`

- **Source:** runtime messages/imports lines 1–38; `WebViewRuntime` launch/open/close/drop lines 145–330; driver implementation from line 349. SHA `61a77d0e0d4ec092e638e4a7a0d93d857126219b`.
- **Purpose:** own UI event-loop thread, WebView, page command channels and cleanup.
- **Dependencies/coupling:** `wry`, `tao`, threads/channels, `PageDriver`, Windows upload helpers and later browser-grid types. File is 3,155 lines, not a small isolated snippet.
- **Evidence/limitations:** page calls have bounded receive timeouts. Drop signals shutdown but deliberately does not join the UI thread, avoiding an indefinite hang at the cost of not proving deterministic teardown. No runtime tests were run. Platform behavior requires target-OS validation.
- **Security/license:** runtime uses a persistent profile directory; ownership must be project-controlled. MIT applies to copied code; transitive licenses remain unaudited.
- **Decision: DO NOT COPY.** Use `wry`/`tao` as independently evaluated third-party crates if appropriate, but build this project's small runtime around its own driver contract. Do not add WebAgent as a dependency to obtain the runtime.
- **Tests:** first prove launch, navigation, real text input and shutdown on target OS; then test renderer hang, command timeout, repeated start/stop and multiple pages if required.

## 4. Provider-specific browser interaction — `src/browser/mod.rs`, `src/browser/backend.rs`

- **Source:** config/profile construction in `browser/mod.rs` lines 90–180; start/stop/readiness/state in `backend.rs` lines 43–193; submit/wait/stream in lines 194–300; login/conversation handling in lines 492–553. SHAs: `f97f3d796ad2f33b9f7151da20a50efbff0f32db`, `532b8ed2c5cd5d23b138e708dfe0ebc25977683a`.
- **Purpose:** selectors, provider-specific send strategies, login state and response detection.
- **Dependencies/limitations:** selector/config registry, PageDriver, WebViewRuntime, BrowserPool, reveal controls and profile overrides. Contains accumulated site-specific fixes; source inspection cannot establish that current web UIs still match.
- **Security:** remote page content is untrusted; profile holds authenticated session material.
- **Decision: DO NOT COPY MODULES.** Build one provider adapter against our own driver; treat upstream quirks as candidate acceptance tests, not reusable implementation.
- **Tests:** logged-in/out, challenge, navigation failure, stale draft, old-response reuse, empty response, generation completion and timeout, with dated live evidence.

## 5. Paths, registry and profile cloning — `src/config/paths.rs`, `src/config/brains.rs`, `src/config/profiles.rs`

- **Source:** path resolution `paths.rs` lines 1–78; Brain/profile mapping `brains.rs` lines 143–218 and 326–350; cloning/lease code `profiles.rs` lines 18–109 and 465–554. SHAs for brains/profiles: `3033a2d86b18bcfb43d476839403c7a70406e940`, `c09090ff90a45137290da1140a5025c91488a2bb`.
- **Purpose:** data root, Brain URL/selectors/profile mapping, profile copies and cleanup.
- **Dependencies/limitations:** `WEBAGENT_ROOT`, `WEBAGENT_PROFILE_DIR`, selector assets, lock files, sparse/full copying, leases and Windows cleanup. Upstream defaults can resolve to `%LOCALAPPDATA%\\webagent`, `~/webagent` or a repository-relative development fallback. Profile cloning solves real cookie/lock/cleanup problems but is a subsystem, not a helper to copy casually.
- **Security:** never silently reuse an existing WebAgent profile; profile data contains secrets.
- **Decision: REIMPLEMENT PATH/REGISTRY; DO NOT COPY CLONING.** Own app data root, per-Brain profile and explicit initialization/sharing policy. Start with one dedicated profile per Brain.
- **Tests:** path precedence, sanitization, disjoint profile paths, persistence after restart, failed-start cleanup, no profile data under repo/build/logs.

## 6. One-turn inference — `src/browser_inference.rs`, `src/relay.rs`

- **Source:** request/response and `complete_streaming_with_attachments` in `browser_inference.rs` lines 1–160 and parser lines 453–530; relay setup in `relay.rs` lines 1–150. Browser inference SHA `c8af0791b523c99f6aefb2a3734f643641da1740`.
- **Purpose:** prompt dispatch, readiness, timeout, response normalization and streaming callback.
- **Dependencies/limitations:** relay, observer, circuit breaker, timeout configuration, attachments and the `WEBAGENT_INFERENCE/1` tool envelope. Tool calls are prompt-driven, not native provider tools; image/media/model-switch paths exceed MVP.
- **Decision: REIMPLEMENT.** Own one-turn text orchestration; no tool envelope, tool execution, attachments or image generation in initial scope.
- **Tests:** empty prompt, one prompt/one reply, readiness error, deadline, cleanup, text normalization and snapshot events; verify no hidden retries/tool execution.

## 7. Browser pool — `src/browser_pool.rs`

- **Source:** ownership model lines 1–120; full file is 902 lines. SHA `c8471d9240229426bdce091cd235e99dee6560af`.
- **Purpose:** shared runtime, pooled tabs, cloned/leased profiles and recovery.
- **Dependencies/limitations:** global `OnceLock<Mutex<_>>`, concrete backend, profile cloning, shared/encapsulated runtime. Source comments document refcount, profile-mixing and teardown edge cases.
- **Decision: REJECT FOR MVP.** One isolated runtime per Brain and serialized same-Brain turns first. Revisit only if measured resource/concurrency needs justify the complexity.
- **Tests:** same-Brain serialization and disjoint profile paths. Pooling would need stress, leak, restart and teardown tests.

## 8. Snapshot delta classifier — `src/observer.rs`, `classify_stream_delta`

- **Source:** lines 14–53; pure string-comparison function, no browser dependencies.
- **Purpose:** distinguish append-only growth, identical snapshots and rewrites.
- **Limitations:** API SSE consumers expect append-style deltas; rewrite behavior needs a project-owned policy. A single final snapshot is not genuine streaming.
- **Decision: REIMPLEMENT THE SMALL ALGORITHM.** It is simple enough to own without importing upstream semantics.
- **Tests:** empty-to-text, prefix growth, identical, shorter/revised text, Unicode, final-only snapshot. Do not claim streaming without multiple changing snapshots.

## Dependency/licensing conclusion

No WebAgent crate dependency is approved. Upstream declares MIT; transitive license closure remains unaudited. Third-party crates such as `wry` and `tao` may be evaluated independently; that is not approval to reuse WebAgent's implementation.

## Next implementation sequence

1. Define the project-owned BrainBackend contract and event/error types.
2. Prove a tiny project-owned WebView runtime with a disposable profile on the target OS.
3. Add one provider adapter and tests.
4. Implement one-turn text orchestration and API separately.
5. Add SSE only after live tests show multiple changing snapshots and rewrite semantics are defined.
6. Defer pooling, tools, media, Responses API and operator UI.

## Evidence status

Source inspected at the pinned revision; file SHAs are recorded above. **No build, upstream tests, live browser session or client-compatibility test was run. Approved WebAgent source reuse: none.**
