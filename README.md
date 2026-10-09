# OpenAI Inference Bridge

A standalone local service that exposes browser-based chat interfaces (“Brains”) through an OpenAI-compatible inference API.

The intended backend is a chat interface that a user is already authenticated to, not a model-provider API and not an autonomous agent harness. Clients such as Pi, OpenCode, OpenClaw, and ZeroClaw should be able to use a configured Brain through a common model endpoint, subject to tested compatibility.

## Status

Planning/bootstrap only. No implementation or compatibility claim is made yet.

## Design principles

- OpenAI-compatible API is the product boundary.
- A Brain is a chat-interface backend; it is exposed as a model ID.
- Keep browser-chat execution separate from agent orchestration, shell execution, and the `webagent/1` action protocol.
- Reuse selected, reviewed components from `webagent-rs`; do not clone unrelated application subsystems.
- Advertise only capabilities verified for each Brain.
- Default to loopback binding and require authentication when exposing the API beyond loopback.
- Keep compatibility claims tied to executable tests.

## Initial API target

First milestone: `GET /v1/models` and `POST /v1/chat/completions`, including SSE streaming. `/v1/responses`, multimodal inputs, tools, cancellation, and other fields must be audited against the existing implementation and assigned an explicit support status before being promised.

## Repository workflow

Start with `START_HERE.md` and `docs/WORK_CONTRACT.md`. Claim tasks in `docs/TASKBOARD.json` before editing. Keep `master` green, use one task per branch, and attach verification evidence under `docs/proofs/`.

## Upstream references

- `st0rax/webagent-rs`: existing browser-based Brain abstraction and local API bridge.
- `st0rax/dummy-bazaar`: project workflow/template inspiration only.

No upstream code is copied into this bootstrap. Reuse decisions and attribution belong in `docs/UPSTREAM_REUSE.md`.