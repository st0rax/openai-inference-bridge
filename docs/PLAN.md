# Project plan — OpenAI Inference Bridge

## Architecture direction

HTTP/OpenAI compatibility layer → normalized request/event types → model/Brain registry → Brain adapter → browser-chat runtime.

The web UI is a thin local control/test surface over the same registry and inference API. It must not become a second inference implementation.

## Milestones

- **Phase P1 — Foundation:** repo structure, scope, license policy, CI and safe defaults.
- **Phase P2 — Upstream audit:** inspect the existing `webagent-rs` API bridge and Brain abstraction; establish a file-by-file reuse decision.
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

## Status handover

- **Completed:** source-level audits `P-010`–`P-017` and file-level reuse decision are documented and merged.
- **Next:** `P-018` integration strategy is documented; then `P-004` clean Rust/CI setup and a pinned-dependency build spike must validate the recommendation.
- **Not verified:** no clean Cargo build, upstream tests or live browser turn has been run from this new repository.
