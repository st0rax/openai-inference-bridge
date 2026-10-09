# Upstream audit — OpenAI Chat Completions compatibility

- **Audit date:** 2026-10-09
- **Upstream revision:** `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22`
- **Method:** source and project-document inspection; tests and live provider claims were not independently rerun.

## Summary

The existing endpoint is a useful compatibility starting point for the baseline `POST /v1/chat/completions`, including model IDs, text messages, selected base64 media, function-tool calls, buffered responses and an incremental text SSE path. It is **not a drop-in semantic clone of OpenAI inference**: the backend is a web-chat turn, usage is unknown, many generation controls cannot be honored, and some unrecognized JSON fields are silently ignored.

The new app should define a precise compatibility subset and reject unsupported semantics rather than accept a field that has no effect.

## Request parsing observed

The root `OpenAiRequest` DTO deserializes only:
- `model`
- `messages`
- `stream`
- `tools`
- `tool_choice`

`ConversationMessage` recognizes `role`, `content`, assistant `tool_calls`, and `tool_call_id`. Unknown fields are not denied by Serde by default. A separate pre-check rejects a specific list of known unsupported top-level fields: `seed`, `service_tier`, `logit_bias`, `best_of`, `echo`, `suffix`, `top_logprobs`, and `response_format`; it rejects `n` unless it is 1 and rejects active `logprobs`.

**Compatibility gap:** fields outside that explicit rejection list can pass deserialization while having no effect. Examples that require deliberate policy include `temperature`, `top_p`, `max_tokens` / `max_completion_tokens`, `stop`, `presence_penalty`, `frequency_penalty`, `logit_bias` variants, `stream_options`, `parallel_tool_calls`, and provider-specific extensions. This list is illustrative, not an exhaustive audit of every SDK field. The DTO does not carry most of these fields into inference. The new API must either implement them, explicitly reject them, or publish them as unsupported in its machine-readable capability contract.

## Messages and roles

The prompt normalizer accepts `system`, `developer`, `user`, `assistant`, and `tool` roles. The last message in the clean browser text profile must be `user` or `tool`. A single user message is passed through as the prompt without a transport wrapper. Multi-message context is rendered into a textual transcript; assistant history is labeled as the model's own prior output, tool calls and tool-call IDs are included as textual history, and historical attachments are represented by markers rather than uploaded again.

This preserves conversation context approximately, but it is not native provider-side message state. The web-chat UI receives a synthesized prompt, so token accounting, role priority, exact context-window behavior and system/developer isolation cannot be assumed equivalent to OpenAI's API.

## Input content

Observed accepted paths include:
- plain string content;
- content arrays with text-like parts;
- `image_url` whose URL is an accepted base64 data URL;
- `input_audio` base64 with an accepted audio format;
- supported assistant tool-call history and tool-result messages.

The bridge decodes base64 media into `BrowserAttachment` values and uploads them through the browser UI. It does not fetch remote `http(s)` media URLs or resolve `file_id` references. Unsupported content parts are rejected rather than silently dropped. Actual media support is browser/UI/Brain-dependent and must be tested per configured Brain.

## Function tools

The endpoint normalizes function definitions and `tool_choice` modes (`auto`, `none`, `required`, or a selected function), then asks the web model to emit the internal `WEBAGENT_INFERENCE/1` envelope. That envelope is parsed into ordinary assistant `tool_calls`. The bridge does not execute functions; the client/harness executes them and submits the result in a subsequent request.

Tool definitions may have descriptions shortened to stay under the browser prompt schema budget. This is materially different from provider-native tool calling. A tool capability is only established after a full round-trip test with a specific Brain/client. Tool-enabled streams are buffered until the returned call has been validated.

## Response behavior

### Buffered response

The handler returns `object="chat.completion"`, a generated completion ID, the requested model ID, one choice, message content or tool calls, a finish reason and a usage object. The source explicitly returns `prompt_tokens=0`, `completion_tokens=0`, `total_tokens=0` because the browser UI does not expose true token counts. These values are placeholders, not actual usage.

The handler returns one choice; `n>1` is rejected. The model's actual context and generation limits are not measured per request. Model metadata uses conservative common defaults, not verified provider token limits.

### Streaming response

- Pure text requests with `stream=true` and no active tools use an incremental handler that emits growing text snapshots as deltas, keepalive comments and a terminal `[DONE]`.
- Requests with active tools do not use the incremental browser path. They wait for a validated result and then serialize the completed answer as an SSE stream. This meets the outer streaming shape but does not provide low-latency deltas for tool-enabled turns.
- Snapshot deltas assume earlier text remains an exact prefix. If the UI edits prior text, the protocol has no way to retract already emitted characters.
- A failure after SSE headers cannot change the HTTP status. The inspected error path emits an in-band error chunk with `finish_reason="error"`; that path needs explicit SDK compatibility tests and a defined terminal-event policy.
- Client disconnect does not propagate a cancellation token through the browser-inference/relay stack; work can continue until the turn ends or times out.

## Error mapping and security boundary

The handler authorizes before processing provider requests, rejects malformed/unsupported inputs, returns 404/model-not-found for unknown model IDs, and maps browser inference errors to provider/server errors in buffered responses. The listener is loopback-only and requires an API token for provider routes. `/health` is intentionally unauthenticated; model listing is authenticated.

A new service should preserve this local-only default. If remote access is needed, add an explicit security boundary (TLS/reverse proxy/auth/rate limits) rather than simply relaxing the bind address.

## Proposed compatibility matrix

| Feature | Source-audit status | Required product policy |
|---|---|---|
| `GET /v1/models` | Implemented; configured IDs plus virtual auto model | Keep only configured and healthy/declared Brains; model listing is not proof of live login |
| Buffered text completion | Implemented | Re-test malformed inputs, errors and message semantics |
| Text SSE | Incremental snapshots for no-active-tool requests | Test terminal events, prefix rewrites, errors and disconnects |
| Roles/system/developer | Normalized to a synthesized browser prompt | Document as approximation; test role ordering and prompt injection boundaries |
| Image/audio inputs | Selected base64 formats are parsed and uploaded | Declare per-Brain capabilities; reject remote URLs/unsupported parts |
| Function tools | Prompt-driven envelope + parser; external client executes | Treat as experimental until client round-trip proof per Brain |
| Token usage | Always zero in Chat Completions response | Mark unavailable; do not present as actual accounting |
| Temperature/top-p/stop/token limits | Not present in DTO/inference request | Reject or explicitly mark unsupported; never silently ignore |
| JSON mode / response_format | Explicitly rejected by current pre-check | Keep rejected unless enforceable and verified |
| Multiple choices (`n>1`) | Rejected | Explicit limitation |
| Cancellation | Not propagated to browser turn | Design before claiming cancellation support |
| Remote media URLs / file IDs | Not fetched/resolved | Reject explicitly |
| Full OpenAI parity | Not established | Do not claim |

## Tests to port or add

- A schema/field-policy test that enumerates accepted, implemented, rejected and unknown fields.
- Tests proving every advertised parameter changes behavior or is rejected.
- Roles and multi-turn transcript fixtures, including assistant tool calls and `role=tool` follow-up.
- Base64 image/audio decode tests and remote URL/file ID rejection.
- Tool-call round trip, malformed/unknown/duplicate call and `tool_choice=required` cases.
- Buffered response schema, finish reason and usage-unavailable semantics.
- Text stream delta order, normal `[DONE]`, quiet keepalive, rewritten-prefix behavior, post-header failure and disconnect.
- Real OpenAI-compatible SDK smoke tests against a mock Brain, followed by a separately labeled live browser smoke for each Brain.

## Verification status

- **Source-verified:** DTO fields, known unsupported-field list, role checks, media handling policy, tool normalization, response fields and streaming route policy.
- **Not verified:** local unit-test execution, SDK behavior, live browser behavior or all OpenAI spec fields. The project documentation reports selected Pi/OpenAI SDK smoke results, but those were not rerun for this audit.
