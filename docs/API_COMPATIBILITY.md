# API compatibility policy

## Baseline target

First target is OpenAI-style:

- `GET /v1/models`
- `POST /v1/chat/completions`
- `stream: true` using Server-Sent Events

The existing `webagent-rs` bridge documents additional endpoints, including `/v1/responses`. Their exact behavior must be verified from implementation and tests before this project promises them.

## Compatibility levels

- **L0 — absent:** not implemented.
- **L1 — parse-only:** accepts fields but semantics may not be honored; must not be advertised as compatible.
- **L2 — basic:** valid baseline request/response works.
- **L3 — streaming:** event framing, deltas, finish and error behavior verified.
- **L4 — client tested:** a real SDK/harness integration has reproducible evidence.
- **L5 — documented feature parity:** relevant endpoint/field semantics have tests and known limitations are explicit.

Do not use “fully compatible” as a blanket label. Publish a per-endpoint/per-feature matrix.

## Candidate features to audit

Model listing, roles/messages, system/developer/user/assistant messages, multiple choices, max tokens fields, stop sequences, temperature/top-p, tool definitions and tool calls, JSON response formats, usage reporting, finish reasons, SSE ordering, errors, timeouts/cancellation, images/audio, Responses input/output items, response retrieval/deletion and conversation state.

A field may be accepted only if its semantics are honored or its limitations are explicitly documented.
