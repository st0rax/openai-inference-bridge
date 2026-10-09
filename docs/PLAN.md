# Project plan — OpenAI Inference Bridge

## Architecture direction

HTTP/OpenAI compatibility layer → normalized request/event types → model/Brain registry → Brain adapter → browser-chat runtime.

The web UI is a thin local control/test surface over the same registry and inference API. It must not become a second inference implementation.

## Milestones

- **Phase P1 — Foundation:** repository structure, scope, license policy, CI and safe defaults.
- **Phase P2 — Upstream audit and component evaluation:** source-level discovery audits are complete; individual component approvals remain outstanding. The earlier public-API-bridge recommendation is superseded.
- **Phase P3 — Independent architecture/API core:** define our own BrainBackend contract, then implement HTTP/auth/model discovery/chat completions/streaming.
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

- **Verified:** Rust scaffold and CI; GitHub Actions passed formatting, Clippy, and tests on Ubuntu and Windows. Safe environment-based configuration, token redaction, loopback-first binding, remote opt-in, per-user data paths, and Brain profile path validation are implemented.
- **Documented:** independent BrainBackend contract, model ID and registry policy, capability declarations, normalized request/events, conversation state, configuration, and secret handling.
- **Not implemented:** HTTP server/routes, request authentication, structured API errors, model registry code, `/v1/models`, Chat Completions, and browser runtime.
- **Binding decision:** implement the OpenAI-compatible API and internal Brain contract in this repository. Do not use WebAgent's public API bridge as the core. Evaluate every candidate upstream component individually before any reuse or dependency adoption.
- **Next:** implement HTTP routing, then authentication/errors and model discovery; follow with normalized non-streaming Chat Completions. In parallel, test a minimal project-owned WebView runtime with an app-owned disposable profile.

See [the detailed status handover](STATUS_2026-10-09.md), [configuration reference](CONFIGURATION.md), and [secret-handling policy](CONFIGURATION_AND_SECRETS.md).
