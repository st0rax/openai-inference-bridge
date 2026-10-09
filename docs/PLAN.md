# Project plan — OpenAI Inference Bridge

## Architecture direction

HTTP/OpenAI compatibility layer → normalized request/event types → model/Brain registry → Brain adapter → browser-chat runtime.

The web UI is a thin local control/test surface over the same registry and inference API. It must not become a second inference implementation.

## Milestones

- **Phase P1 — Foundation:** repository structure, scope, license policy, CI and safe defaults.
- **Phase P2 — Upstream audit:** inspect the existing `webagent-rs` API bridge and Brain abstraction; establish a file-by-file reuse decision. **Source-level audit complete.**
- **Phase P3 — Architecture/API core:** freeze internal contracts and implement HTTP/auth/model discovery/chat completions/streaming.
- **Phase P4 — Brain runtime and compatibility:** connect a browser-chat Brain, then verify clients and protocol behavior.
- **Phase P5 — UI and hardening:** local chat diagnostics, security, privacy, observability and failure tests.
- **Phase P6 — Release:** setup guide, support matrix, reproducible package and clean-checkout acceptance.

Task dependencies are authoritative in `docs/TASKBOARD.json`.

## Architecture constraints

- The API layer must not depend on the agent controller, shell executor, autonomous loop, or `webagent/1` action protocol.
- Browser runtime code must not parse OpenAI wire DTOs directly.
- Provider wire formats must normalize into internal types before runtime dispatch.
- The model catalog must report only configured and actually available Brains.
- Streaming must have explicit cancellation, error and terminal-event semantics.
- Default network binding is loopback. Non-loopback exposure requires authentication and explicit configuration.
- Set an application-specific `WEBAGENT_ROOT` before upstream configuration/runtime initialization; profile sharing must never happen implicitly.

## Status handover — 2026-10-09

- **Done:** repository bootstrap and planning; source-level audits for API bridge modules, Brain lifecycle, inference/streaming, Chat Completions, Responses API, media capabilities, and the file-level reuse decision; integration strategy and acceptance gates.
- **Recommendation:** use a pinned `webagent-rs` library dependency and validate it in a clean build spike. Do not copy `api_bridge` or `brain.rs` in isolation because they depend on the upstream runtime/config/profile/session stack.
- **Next:** set up the Rust workspace and CI (`P-004`), then prove the pinned dependency builds and can serve a buffered and streaming text request.
- **Not verified:** no clean Cargo build, upstream tests, live browser turn, or client compatibility test has been run from this new repository.

See [the detailed status handover](STATUS_2026-10-09.md) and the audit reports linked there.
