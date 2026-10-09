# Upstream reuse decision — 2026-10-09

- **Decision status:** recommended for prototype; dependency build/runtime spike still required.
- **Reviewed upstream:** [st0rax/webagent-rs](https://github.com/st0rax/webagent-rs), `master` tree `a6693dcc8095b3306a11593a216741f8a5c85a22`, package version `0.11.3`.
- **License:** upstream repository declares MIT and includes the corresponding license text. No source files copied in this audit. Transitive dependency license closure has not been audited.

## Decision

**Prototype with a pinned `webagent` library dependency and a small application wrapper. Do not copy the API bridge or browser runtime source into this repository yet.**

The upstream library exposes `webagent::api_bridge::BridgeConfig` and `webagent::api_bridge::serve` publicly. The existing API bridge already provides a harness-free browser-inference path and has a tested source-level boundary. Reusing that implementation is less risky than reimplementing browser selectors, profile lifecycle, upload interactions, WebView startup, response detection and recovery.

Pin the reviewed revision, not floating `master`. The first implementation spike should prove that the dependency builds from a clean checkout on the target OS, can load a disposable Brain profile, starts the API on loopback, and passes model-list + text-turn + streaming smoke tests. Only after this evidence should the dependency approach be treated as accepted architecture.

This still produces a separate executable/repository that clients can call over the API. It does **not** mean the implementation is source-independent of WebAgent. If source-level independence becomes a hard requirement, a later extraction is needed.

## File-level decision matrix

| Source path / area | Decision | Reason and constraints |
|---|---|---|
| `src/api_bridge.rs` | **Reuse through public library API for prototype; do not copy** | Composition root owns DTOs, server, limits, routes, image/audio handlers and session integration. It is not a standalone source file. |
| `src/api_bridge/boundary.rs` | **Reuse as part of upstream API; port tests if extracting** | Auth and error mapping are useful. Keep bearer token secret, timing-aware comparison and fail-closed behavior. |
| `src/api_bridge/transport.rs` | **Do not copy into a new server** | Bespoke blocking HTTP parser with narrow framing support. If a fresh HTTP layer is eventually built, use a maintained HTTP framework and port boundary tests. |
| `src/api_bridge/wire.rs` | **Reuse upstream for prototype; later adapt semantics** | HTTP/SSE formatting and Responses event sequencing are already centralized; use framework streaming primitives if the API layer is rewritten. |
| `src/api_bridge/routing.rs` | **Port small logic only if extracting** | Clear method/path routing policy; simple to test, but not worth maintaining a duplicate while calling the upstream server. |
| `src/api_bridge/provider_handlers.rs` | **Reuse upstream for prototype; port behavior/tests, not file verbatim** | Contains valuable OpenAI/Anthropic/Responses handling but depends on many parent-level types and helpers. |
| `src/api_bridge/content.rs` | **Port behavior/tests only if extracting** | Message/content normalization, tool schema and media decoding; needs new internal types and explicit unknown-field policy. |
| `src/api_bridge/inference.rs` | **Reuse upstream for prototype; retain adapter concept** | Per-Brain serialization and single-turn calls are key invariants. Includes upstream AutoRouter/timeouts and test double. |
| `src/api_bridge/catalog.rs` | **Replace with a configuration-driven registry in this app if a separate API layer is built** | Hard-coded Brain IDs, AutoRouter preferences and modality declarations are upstream-specific. Capabilities should be explicit and evidence-based. |
| `src/api_bridge/response_protocol.rs` | **Reuse upstream for prototype; port only the formats we promise** | Response serialization is useful, but Responses has known state/field discrepancies documented in the audit. |
| `src/api_bridge/store.rs` | **Defer in MVP; do not copy** | Responses persistence is not required for Chat Completions. Source persists JSON to disk while docs say memory-only; response `store` is hardcoded true. Resolve before reuse. |
| `src/api_bridge/tests.rs` | **Port selected tests and fixtures during extraction** | Strong behavioral inventory, but test module is coupled to parent private types. Tests were inspected, not executed in this audit. |
| `src/brain.rs` | **Preserve as conceptual contract; do not copy alone** | `BrainBackend` is a good lifecycle abstraction but does not implement a runtime by itself. |
| `src/browser_inference.rs` | **Reuse via dependency for prototype; likely long-term adapter boundary** | Normalized request/result, attachment validation, streaming and internal tool envelope. Still depends on relay, observer and diagnostic modules. |
| `src/relay.rs` | **Reuse via dependency; do not copy** | Owns turn retries, timeouts, circuit breaker, scoring, proof and diagnostic integration. Extraction would need dependency reduction. |
| `src/browser/mod.rs`, `src/browser/backend.rs` | **Reuse via dependency; do not copy** | Concrete Brain implementation depends on selectors, configuration, WebView, profile pool, observer and UI operations. |
| `src/browser_pool.rs` | **Reuse via dependency; do not copy** | Shared runtime, tab refs, fallback profiles, cleanup and UI-window integration form a subsystem. |
| `src/webview_runtime.rs`, `src/page_driver.rs` | **Reuse via dependency; do not copy** | WebView thread/event-loop and platform-specific page driver are critical runtime components, not API glue. |
| `src/config/brains.rs`, `src/config/profiles.rs`, related config | **Reuse via dependency for prototype; isolate root explicitly** | Brain registry, selectors, profile directories, profile cloning and write-back. Profiles contain authenticated session secrets. |
| `src/session.rs`, `src/observer.rs`, `src/timeouts.rs`, `src/circuit_breaker.rs`, `src/bench_events.rs`, `src/brain_score.rs`, `src/capability_proof.rs` | **Transitive runtime dependencies; keep behind the library for prototype** | Do not selectively copy without defining replacements and the complete dependency closure. |
| `src/main.rs`, CLI/TUI, agent controller, executor, shell policy, `webagent/1`, workers, benchmark/research UI | **Exclude from this app's product boundary** | The inference bridge must not become an agent harness or local tool executor. The upstream crate may compile these modules because its library surface is broad, but the wrapper must not invoke them. |
| `docs/API_BRIDGE.md`, `docs/API_BRIDGE_ARCHITECTURE.md` | **Use as evidence sources, reconcile with source** | Docs contain useful operational and compatibility notes but at least one Responses storage discrepancy exists. Source code at the pinned revision is authoritative for implementation behavior. |
| `LICENSE` | **Retain notice if copying any substantial code** | MIT requires retaining copyright/license text. Git dependency still requires respecting the license; audit transitive dependency licenses before redistribution. |

## Prototype architecture

```text
OpenAI-compatible client / harness
  -> openai-inference-bridge executable (this repo)
      -> local configuration + secret validation + isolated data root
      -> pinned webagent::api_bridge::serve(BridgeConfig)
          -> existing API handlers and SSE
          -> existing browser_inference + relay
          -> Brain registry, profile lifecycle and WebView runtime
  -> separate minimal operator/test UI in a later task
```

The wrapper must remain thin. It should own:
- application-specific configuration and CLI;
- API token validation from environment (never command-line argument or committed config);
- explicit loopback bind and selected Brain/timeout;
- a dedicated `WEBAGENT_ROOT` default for this app;
- startup/shutdown messaging and health/diagnostic presentation;
- client integration smoke scripts and capability/support matrix.

It must not call `AgentController`, `ShellExecutor`, autonomous loops, local tools or the `webagent/1` action protocol.

## Isolation requirement: `WEBAGENT_ROOT`

The upstream path resolver defaults to a stable `webagent` data root (for example `%LOCALAPPDATA%\\webagent` on Windows) and stores profiles under that root. If the wrapper leaves this default unchanged, it may reuse the existing WebAgent installation's profiles and authenticated sessions.

Set `WEBAGENT_ROOT` to a dedicated application-data directory **before any upstream configuration/runtime calls** (for example an app-specific directory under the user's local application data). Sharing an existing profile should be an explicit opt-in, not an accidental default. Never put profile contents under the repository, the build output, a public artifact or CI cache.

## Compatibility and product scope

- Start with `GET /v1/models` and `POST /v1/chat/completions`, including tested text SSE.
- The upstream server currently exposes additional Responses, Anthropic, Images and Audio routes when `serve` is called. Their existence must not be represented as full compatibility; see the audit docs for field and state limitations.
- The direct public `serve` entry point is API-only. The upstream CLI documentation describes a combined UI/API mode, but the new wrapper should not silently invoke the upstream CLI/TUI. A minimal operator/test console can be implemented separately after the API path is proven.
- If the wrapper needs to restrict endpoints to the committed product subset, it needs an explicit route policy/proxy or a future upstream API configuration hook. Do not imply the public `serve` function supports per-route disablement unless verified.

## Required acceptance spike before production code

1. Add a pinned Git dependency at `a6693dcc8095b3306a11593a216741f8a5c85a22`; build from a clean checkout on the target OS.
2. Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings` and tests for the wrapper; run the relevant upstream library tests.
3. Verify the wrapper sets an app-specific `WEBAGENT_ROOT` before upstream config is initialized.
4. Verify missing/weak API token fails startup, token never appears in logs, and only loopback binding is allowed.
5. Start the service with a disposable authenticated Brain profile; test `/health`, `/v1/models`, one buffered text turn and a text SSE turn.
6. Test client disconnect, shutdown, per-Brain serialization, browser login-required state and timeout behavior.
7. Record exact OS, Rust toolchain, upstream revision, commands and redacted output in `docs/proofs/`.
8. Decide whether the compile footprint and broad upstream library dependency are acceptable. If not, open a separate extraction design; do not start an unplanned source copy.

## Evidence links

- [API bridge inventory/call graph](UPSTREAM_AUDIT_2026-10-09.md)
- [Brain runtime lifecycle](BRAIN_RUNTIME_AUDIT_2026-10-09.md)
- [Inference/tool/streaming audit](INFERENCE_STREAMING_AUDIT_2026-10-09.md)
- [Chat Completions compatibility audit](CHAT_COMPLETIONS_AUDIT_2026-10-09.md)
- [Responses API/state audit](RESPONSES_API_AUDIT_2026-10-09.md)
- [Media/capability audit](MEDIA_CAPABILITY_AUDIT_2026-10-09.md)

## Status

This is a source-based architecture recommendation, not a verified build or runtime result. The pinned-dependency spike remains the next gate.
