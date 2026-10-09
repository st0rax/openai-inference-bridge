# Normalized request and event types

Status: P-023 design proposal, 2026-10-09. Types are specified conceptually; Rust implementation follows workspace and review.

## Two explicit layers

Keep API-domain normalization separate from browser-turn execution.

### `NormalizedChatRequest` — API/application domain

- `model_id`: canonical registered ID, such as `oib/chatgpt`.
- `messages`: ordered list of `ChatMessage`.
- `stream_requested`: whether the client requested streaming delivery; this is not a claim that the Brain can stream incrementally.
- `deadline`: one absolute monotonic deadline for the entire request.
- `cancellation`: cloneable project-owned cancellation handle.

`ChatMessage` contains a role and plain-text content. Initial supported roles are `system`, `user`, and `assistant`. Content must be a string; multimodal content arrays, tool messages, function calls, names, and other unsupported fields are rejected explicitly. Empty message lists and requests without a final user message are invalid in the first milestone.

No OpenAI wire DTO is passed to the BrainBackend. The API layer validates incoming JSON, rejects unsupported fields, and creates this normalized type.

### `BrainTurnRequest` — backend boundary

- `prompt`: deterministic, project-composed plain-text prompt representing the ordered message history.
- `deadline`: the same absolute deadline, not a reset timeout.
- `cancellation`: the same cancellation handle.

The prompt composer preserves role distinctions in the rendered transcript and separates the transcript from the instruction to answer the final user message. It must not silently drop earlier assistant/user messages. Because browser chat UIs do not expose a native system role consistently, the system message is represented as an explicit instruction in the composed prompt; this limitation must be documented. Prompt composition is a project-owned function with fixtures and tests.

The BrainBackend receives neither `model_id` nor `stream_requested`: model resolution happens before dispatch and HTTP delivery preference stays in the API layer.

## Events and final result

Use typed internal events; do not encode HTTP/SSE details here.

- `Snapshot { text }`: full currently observed assistant text. It may repeat or revise earlier text.
- `Completed { text }`: authoritative final answer, emitted once after a defensible completion signal.
- `Failed { error }`: terminal typed failure, emitted at most once.

The final method result and terminal event must not contradict each other. A suggested Rust shape is a result-returning backend method plus non-terminal snapshot callbacks; the method return remains authoritative. Do not send both a terminal `Failed` event and a successful return.

The event sink returns `Continue` or `Cancel`. `Cancel` requests cooperative cancellation and must stop forwarding events. The request cancellation handle is also checked between bounded browser operations. A disconnect is not proof that the provider stopped generating.

## Snapshot vs delta

Events carry full snapshots, not append deltas. The API adapter compares snapshots:
- identical snapshot: emit nothing;
- new text extends the prior text: only the suffix can be emitted as `delta.content`;
- rewritten/shortened text: ordinary Chat Completions SSE cannot safely retract already-sent content. Until a tested rewrite policy exists, stop streaming with an explicit error or buffer until the final answer. Never append the full revised snapshot and silently corrupt output.

A Brain may be marked `VerifiedAppendOnly` only after live tests demonstrate multiple distinct snapshots that never rewrite the prefix. Otherwise use buffered-only delivery. These rules align with [the capability policy](BRAIN_CAPABILITIES.md).

## Error taxonomy

Use a project-owned enum, with at least:
- invalid request / unsupported request feature (API validation);
- model not found;
- login required;
- challenge detected;
- rate limited;
- deadline exceeded;
- cancelled;
- browser unavailable;
- navigation failed;
- submission failed;
- response not detected;
- response extraction failed;
- unsupported Brain capability;
- internal failure.

The API layer maps API validation and model resolution errors to client responses. Backend errors are mapped by a separate API policy; the BrainBackend must not choose HTTP status codes. Diagnostic messages are safe by default and must not include credentials, profile data, or complete prompts/responses.

## Deadlines and cancellation

- Deadline is absolute and monotonic, not a duration restarted by each layer.
- Before readiness, submission, each wait/poll and cleanup, check remaining time/cancellation where safe.
- Timeouts must identify the stage that expired.
- On client disconnect, set cancellation and stop emitting; the worker still owns browser cleanup.
- If a page operation cannot be interrupted, bound that operation and document cancellation latency. Do not block a Tokio executor thread with synchronous WebView work.

## Validation and invariants

1. Incoming OpenAI DTOs are converted once at the API boundary.
2. Unsupported fields/content are rejected rather than ignored.
3. Exact model ID resolves to one configured Brain before browser startup.
4. One absolute deadline is shared by all stages.
5. Snapshot events are full text and are never mislabelled as deltas.
6. A successful final result contains authoritative final text and has a defensible completion signal.
7. Terminal completion/failure occurs once.
8. Same-Brain requests are serialized by the worker manager.
9. No tools, attachments, media, provider model switching, or Responses API semantics are implied.

## Required tests

- role/content normalization and prompt-composition fixtures, including multiple turns;
- unsupported roles and non-string content rejected;
- final-user-message requirement;
- exact model resolution before browser work;
- one deadline shared across all stages;
- identical, append-only, shortened and rewritten snapshots;
- event sink cancellation and disconnected client;
- terminal event/result consistency;
- error-to-API mapping tests in the API layer;
- no secret or full prompt/response leakage in diagnostics.

## Related design documents

- [BrainBackend contract](BRAIN_BACKEND_CONTRACT.md)
- [Brain registry and model IDs](BRAIN_REGISTRY_MODEL_IDS.md)
- [Per-Brain capabilities](BRAIN_CAPABILITIES.md)
