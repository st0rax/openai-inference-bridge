# OpenAI Inference Bridge

A standalone local service that exposes browser-based chat interfaces (“Brains”) through an OpenAI-compatible inference API.

The intended backend is a chat interface to which a user is already authenticated—not a model-provider API and not an autonomous agent harness. Clients such as Pi, OpenCode, OpenClaw, and ZeroClaw are intended targets, but compatibility is not claimed until tested.

## Current status

**Upstream source audit complete; implementation not started.** The audit recommends testing a pinned Rust library dependency on `st0rax/webagent-rs` rather than copying its API bridge and browser runtime source files. This recommendation is not yet validated by a build or live browser test.

Start with [the current status handover](docs/STATUS_2026-10-09.md), then read [the integration strategy](docs/INTEGRATION_STRATEGY_2026-10-09.md) and [the file-level reuse decision](docs/UPSTREAM_REUSE_DECISION_2026-10-09.md).

## Design principles

- The OpenAI-compatible API is the product boundary.
- A Brain is a chat-interface backend exposed as a model ID.
- Keep browser-chat execution separate from agent orchestration, shell execution, and the `webagent/1` action protocol.
- Reuse only reviewed components; do not clone unrelated application subsystems.
- Advertise only capabilities verified for each Brain.
- Bind to loopback by default; require authentication for explicitly configured non-loopback exposure.
- Tie compatibility claims to executable tests.

## Initial API target

The first milestone is `GET /v1/models` and `POST /v1/chat/completions`, including SSE streaming. The upstream audit documents important compatibility gaps and limitations for request fields, usage, tool calls, streaming cancellation, Responses API state, and media. Do not promise those behaviors without explicit policy and tests.

## Repository workflow

Start with `START_HERE.md` and `docs/WORK_CONTRACT.md`. Claim tasks in `docs/TASKBOARD.json` before editing. Keep `main` green, use one task per branch, and attach verification evidence under `docs/proofs/`.

## Upstream references

- `st0rax/webagent-rs`: existing browser-based Brain abstraction and local API bridge.
- `st0rax/dummy-bazaar`: workflow/template inspiration only.

No upstream source is copied into this repository. Reuse decisions and attribution are recorded in `docs/UPSTREAM_REUSE.md`.
