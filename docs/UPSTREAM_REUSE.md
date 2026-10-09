# Upstream reuse decision

**Current decision:** prototype with a pinned `st0rax/webagent-rs` library dependency; do not copy bridge/runtime files yet. The reviewed source revision is `a6693dcc8095b3306a11593a216741f8a5c85a22` (package `0.11.3`). This remains conditional on a clean build and live-browser acceptance spike.

The full file-level decision matrix, dependency constraints, isolation requirements and acceptance gate are in [`UPSTREAM_REUSE_DECISION_2026-10-09.md`](UPSTREAM_REUSE_DECISION_2026-10-09.md).

Detailed source audits:
- [API bridge inventory and call graph](UPSTREAM_AUDIT_2026-10-09.md)
- [Brain runtime lifecycle](BRAIN_RUNTIME_AUDIT_2026-10-09.md)
- [Inference, tool calling and streaming](INFERENCE_STREAMING_AUDIT_2026-10-09.md)
- [Chat Completions compatibility](CHAT_COMPLETIONS_AUDIT_2026-10-09.md)
- [Responses API and state lifecycle](RESPONSES_API_AUDIT_2026-10-09.md)
- [Media paths and capabilities](MEDIA_CAPABILITY_AUDIT_2026-10-09.md)

## Decision in one paragraph

The public `webagent::api_bridge::serve(BridgeConfig)` boundary can back a separate executable without tying the product to Pi/OpenCode/OpenClaw/ZeroClaw. The runtime is too coupled to profile/configuration/WebView/relay modules for a safe small-file copy. Pin the upstream revision for the prototype, set an app-specific `WEBAGENT_ROOT` by default to avoid accidentally sharing authenticated profiles, and keep the wrapper limited to configuration, API lifecycle, diagnostics and later operator UI. If a separate source/runtime implementation becomes necessary, design a dedicated extraction rather than copying `src/api_bridge/*.rs` or `src/brain.rs` in isolation.

## Known caveats

- Source audit only; no clean build or live-browser test has been run for this new repository.
- The upstream repository declares MIT. Keep notices if copying code; transitive dependency license review remains outstanding.
- Chat Completions accepts only a subset of OpenAI semantics; some fields may be silently ignored.
- Responses source and documentation disagree about persistence; response objects hardcode `store: true`.
- Media capabilities vary by Brain and operation; the Speech route currently fails closed without a verified TTS artifact.
