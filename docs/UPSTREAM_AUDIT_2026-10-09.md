# Upstream audit — API bridge inventory and call graph

- **Audit date:** 2026-10-09
- **Repository:** [st0rax/webagent-rs](https://github.com/st0rax/webagent-rs)
- **Audited ref:** `master`, tree/commit `a6693dcc8095b3306a11593a216741f8a5c85a22`
- **Purpose:** identify the existing bridge boundary and dependency edges before any source reuse.
- **Method:** GitHub repository tree and source/document inspection. This was a source audit, not a local build or live-browser test. No upstream files were copied or changed.

## Executive finding

The upstream already contains a real, wired, harness-free browser-inference API bridge. It is not merely a design document or a set of orphan modules. `src/api_bridge.rs` declares and wires the child modules, starts a loopback TCP listener, routes requests and calls `browser_inference` for a single model turn. `docs/API_BRIDGE_ARCHITECTURE.md` explicitly records the module wiring and its integration history.

The key reusable *behavioral seam* is:

```text
OpenAI/Anthropic HTTP request
  -> api_bridge transport + routing + provider handler
  -> normalized PromptBundle / BrowserTool / BrowserAttachment
  -> api_bridge::inference (per-Brain lock)
  -> browser_inference::complete[_streaming]_with_attachments
  -> relay::relay_single_turn_with_attachments_streaming
  -> BrainBackend / browser WebView runtime
  -> normalized BrowserInferenceResponse
  -> provider response / SSE
```

The bridge deliberately does **not** call `AgentController`, run a local tool, or interpret the external `webagent/1` task/action protocol. Function-tool calls are represented in the response for the calling harness to execute. That separation matches this project's product boundary.

However, the source is **not a self-contained crate**. Most API modules depend on private types/helpers in the parent `api_bridge.rs`; inference and catalog call other `webagent` modules; the browser runtime reaches into configuration, browser pooling, session state, observer, relay, timeout and WebView modules. Copying only `src/api_bridge/*.rs` would not build. Copying the runtime alone would also not be a small extraction.

**Initial recommendation:** do not copy files yet. Evaluate a pinned dependency on the existing `webagent` library as the shortest path to a working standalone application, then decide whether a smaller upstream library boundary is worth the maintenance cost. A fully independent runtime extraction is a separate architectural project, not a routine file copy.

## Repository and build boundary

| Item | Audited fact | Implication |
|---|---|---|
| Package | `Cargo.toml`: package `webagent`, version `0.11.3`, Rust 2021, library crate name `webagent` and binary `webagent` | It can be consumed as a Rust library, but it is primarily a broader application package |
| Library surface | `src/lib.rs` publicly exports `api_bridge`, `browser_inference`, `brain`, `browser`, `browser_pool`, `config`, `relay`, `session`, `timeouts`, `webview_runtime`, and many unrelated subsystems | Public access exists, but the actual dependency closure is broad |
| Features | Default feature is `webview`; `tui` is optional | Real browser inference needs the WebView feature; `default-features = false` is not a viable real-runtime configuration without further refactoring |
| Platform | `wry`/`tao` WebView stack; Windows-specific WebView2 bindings and native libraries; Linux uses the WebView stack's platform dependencies | CI and packaging need OS-specific setup; do not assume a headless browser is a platform-neutral pure-Rust component |
| Build script | `build.rs` is configured; Windows resources are used | A git dependency may bring build-time/platform constraints even if the new app has a tiny `main.rs` |
| License | Upstream `LICENSE` is MIT, copyright st0rax (2026) | Source reuse is permitted under MIT terms; retain copyright/license notices for copied substantial portions. This audit does not yet inventory every transitive dependency's license |
| Source snapshot | Audited `master` tree: `a6693dcc8095b3306a11593a216741f8a5c85a22` | Pin this exact revision for a prototype; do not depend on floating `master` |

## API bridge module inventory

The current root declares all of the following modules. The architecture doc says these modules are wired in the audited tree; this is important because historical source files without a `mod` declaration were explicitly treated as unwired.

| Path | Responsibility / dependencies observed | Reuse assessment |
|---|---|---|
| `src/api_bridge.rs` | Public `BridgeConfig` and `serve`; HTTP request/response and API DTO types; constants and request limits; accept loop, connection limiter, route dispatch; image generation and multipart audio handlers; `SessionService` integration. Calls `browser_inference`, `relay`, `config`, `observer`, `session`. | **Do not copy wholesale.** It is the composition root and owns more than the baseline Chat Completions API. Use as-is through the library for a prototype, or split/port intentionally later. |
| `boundary.rs` | Bearer / `x-api-key` authentication, equal-length byte comparison, OpenAI/Anthropic error bodies and model-not-found errors; depends on parent DTOs. | **Adapt selectively.** Preserve fail-closed auth and error semantics; avoid coupling the new API DTOs to parent private types. |
| `transport.rs` | Custom blocking HTTP/1.x parser over `TcpStream`, `Content-Length`, request cap, rejects non-identity transfer encoding; depends on parent request type/constants. | **Prefer replacing** with a maintained HTTP server library in a fresh implementation. Reusing this parser means owning protocol edge cases and request-smuggling/timeout review. |
| `wire.rs` | HTTP response serialization, JSON headers, SSE headers/frames, Responses `sequence_number`; depends on parent response type and ID helper. | **Adapt semantics, not necessarily code.** Use framework response/SSE facilities where possible and retain protocol-specific tests. |
| `routing.rs` | Method/path classification and selection of incremental streaming only for `stream=true` with no active tools (or `tool_choice=none`). | **Good small candidate to port** after defining the new endpoint contract. |
| `provider_handlers.rs` | OpenAI Chat Completions (buffered and incremental), Anthropic Messages, Responses API; calls auth, normalizers, inference, response serializers, store and `SessionService`. | **Adapt selectively.** It contains valuable compatibility behavior but is coupled to many parent-level functions and DTOs. |
| `content.rs` | JSON decode; unsupported-field rejection; messages/prompt normalization; OpenAI/Anthropic/Responses tool parsing; base64/data URL attachment conversion. | **Strong candidate for behavioral reuse**, but port behind new normalized internal types and keep the fail-closed validation tests. |
| `inference.rs` | Browser run locks keyed by lowercase Brain ID; blocking and streaming single-turn invocation; auto-route timeout handling; fake response hook for tests. Calls `browser_inference` and `timeouts`. | **Reuse the concept / adapter contract.** The per-Brain lock is important; test-double and auto-routing code can be separated. |
| `catalog.rs` | Brain IDs, model resolution, metadata, modality declarations, heuristic AutoRouter; calls `config::brains` and circuit breaker. | **Adapt.** Model IDs and registry are useful; hard-coded Brain names and capabilities are upstream-specific and must be replaced with configured adapters plus verified capability declarations. |
| `response_protocol.rs` | OpenAI/Anthropic/Responses JSON objects and SSE payloads. | **Selective reuse** if those formats remain in scope; do not pull in Responses merely because the source supports it. |
| `store.rs` | Responses lifecycle, prior-response context, per-token tenant namespace, local JSON persistence and eviction. Calls `config::data_dir`. | **Defer for Chat Completions MVP.** Needed only if Responses retrieval/state is a committed feature. Review retention, permissions, atomicity and error handling before reuse. |
| `tests.rs` | API bridge unit tests, protocol behavior and helper fixtures. | **Port test cases/fixtures**, not blindly the parent-coupled test module. Preserve behavioral regression coverage. |

### Parent-level types and root-only handlers

The following currently live in `src/api_bridge.rs`, not as independent module APIs:

- `BridgeConfig`, `HttpRequest`, `HttpResponse`, `ApiFlavor`, request DTOs and normalized message/tool types.
- `MAX_REQUEST_BYTES = 16 MiB`, `READ_TIMEOUT = 30s`, and `MAX_CONCURRENT_CONNECTIONS = 8`.
- Image generation and audio multipart handling, including the speech route.
- Shared route orchestration and session-event wiring.

The source sets socket read/write timeouts to 30 seconds. The accept loop uses a thread per accepted connection, with an eight-connection limiter; browser runs are additionally serialized per Brain. A fresh implementation should distinguish request-body read timeout, inference deadline, and streaming idle timeout rather than treating them as one setting.

## Cross-module dependency closure

Observed direct references establish these edges:

- `api_bridge.rs` → `api_bridge::{boundary,catalog,content,inference,provider_handlers,response_protocol,routing,store,transport,wire}`.
- `catalog.rs` → `config::brains`, `circuit_breaker::check`, and browser attachment types.
- `inference.rs` → `browser_inference`, `timeouts`, and catalog AutoRouter.
- `browser_inference.rs` → `relay::relay_single_turn_with_attachments_streaming`, `observer::chat_answer_text`, `bench_events`.
- `relay.rs` → `brain`, `browser::WebBrainBackend`, `timeouts`, `circuit_breaker`, `brain_score`, `capability_proof`, `bench_events`, `observer`.
- `browser/backend.rs` → `brain::BrainBackend`, `page_driver`, `webview_runtime`, `config`, `browser_pool`, `bench_events`.
- `browser/mod.rs` → `page_driver`, `brain`, `webview_runtime`, `config`, `webview_reveal`, `browser_pool`, `observer`, `protocol`.
- `browser_pool.rs` → shared/encapsulated profile configuration, browser tabs, WebView runtime, UI layout/window helpers and event logging.
- `webview_runtime.rs` → `page_driver`, upload JavaScript and platform/window helpers.

The `BrainBackend` trait in `src/brain.rs` is a useful conceptual abstraction (start/stop/readiness/send/wait/streaming/conversation restore). It is **not by itself a reusable browser engine**: its concrete implementation is `WebBrainBackend`, which relies on the broader WebView/profile/runtime graph. Also, the current API inference path goes through `browser_inference` and `relay`, rather than calling the trait directly.

## API contract facts to carry forward

- Documented local default is `127.0.0.1:8788`; only loopback addresses are accepted. Provider endpoints require a token.
- Supported families include Chat Completions, model listing, Responses, Anthropic Messages, Images, Audio Transcriptions/Translations, and a Speech route that currently fails closed without an extractable TTS artifact.
- Chat Completions supports text and selected base64 image/audio inputs; external media URLs and unsupported blocks are rejected rather than fetched/ignored.
- Function tool definitions and tool-call messages are translated for the browser model, but the bridge **does not execute tools**. The external harness owns tool execution.
- Token usage is reported as zeros because the browser UI does not expose real token counts. Do not treat those zeros as measured usage.
- Unsupported request fields such as `seed` and `response_format` are rejected. This is safer than silently pretending to honor them.
- Streaming uses a dedicated incremental path for text streams without active tools; tool-enabled requests are buffered until the tool envelope is validated. Streaming errors after headers are sent cannot be converted into a normal non-2xx HTTP response.
- Responses state is stored locally under a tenant-derived directory; the baseline Chat Completions path does not need this state store.
- The docs report a live Pi text-turn and a real Pi `read` tool-loop smoke for ChatGPT. That is useful evidence for that particular Brain/client/path, not universal evidence for all Brains or all OpenAI-compatible clients.

## Security and correctness items to keep visible

1. Preserve loopback-only default and bearer authentication. If adding non-loopback support, treat it as an explicit security design change, not a bind flag.
2. Keep secrets out of command-line arguments, logs, source control and browser diagnostics.
3. Bound request bytes, attachment bytes, concurrency, inference time and stream writes independently.
4. Test malformed HTTP framing, duplicate headers, invalid/overflowing Content-Length, slow clients, disconnects during SSE, cancellation and server shutdown. A custom parser needs especially careful review.
5. Keep unsupported fields fail-closed; never claim schema-constrained JSON, token counts, tools or modalities without semantics/evidence.
6. Browser profiles and authenticated sessions are sensitive state. Do not copy profiles or session stores into this repo or test fixtures.
7. Verify upstream commit, license and dependency licenses before copying code; pin any git dependency to a reviewed revision.

## Decision gate

Do **not** start copying `src/api_bridge/*.rs` as a first implementation step.

The next audit tasks must settle (a) browser/session lifecycle and platform constraints, (b) exact inference/locking and streaming behavior, (c) Chat Completions semantics, (d) Responses state semantics, and (e) modality/capability proof. Then compare two implementation routes:

1. **Pinned library dependency (recommended prototype):** new executable depends on `st0rax/webagent-rs` at the audited revision and calls its public `webagent::api_bridge::serve(BridgeConfig)`. This reuses the actual runtime and avoids a brittle source copy, but keeps a large upstream dependency and requires a wrapper for configuration, health/UI, packaging and upgrade control.
2. **Independent extraction (longer-term):** move/port the API DTO/HTTP layer into this repository and create a narrow runtime adapter. This is not currently possible by copying a few files; it needs an explicit dependency-closure reduction or a separately maintained runtime boundary.

No route or behavior is certified by this source audit alone; run build/test gates and real-browser smoke tests before making release claims.
