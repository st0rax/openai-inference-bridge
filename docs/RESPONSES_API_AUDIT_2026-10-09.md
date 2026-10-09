# Upstream audit — OpenAI Responses API

- **Audit date:** 2026-10-09
- **Upstream revision:** `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22`
- **Method:** source and documentation inspection. No SDK or live-browser tests were rerun.

## Summary

The upstream contains a substantial Responses-shaped adapter: string/message input, instructions, selected image/audio content, function tools and function-call outputs, streaming event objects, `previous_response_id`, retrieval, input-item listing and deletion. It is still a synthesized browser-chat transcript behind an API-shaped envelope, not a native OpenAI Responses runtime.

There are two concrete discrepancies in the audited revision that must be resolved before claiming compatibility:

1. **Storage behavior conflicts with the docs.** `src/api_bridge/store.rs` loads and writes `{data_dir}/openai-local-state-v1/{tenant}/store.json`, so response state persists to disk. `docs/API_BRIDGE.md` says the store is in-memory and IDs disappear on process restart. Source behavior and documentation disagree.
2. **The response object hardcodes `"store": true`.** `response_object` and `response_object_from_answer` set this value unconditionally. The handler skips calling `store_response` when request `store=false`, but the returned object can still claim `store=true`. This is a response contract bug, not just a documentation issue.

Do not port Responses support into the first milestone by default. Chat Completions is a smaller and more directly useful first API surface; Responses should be enabled only after its storage and lifecycle semantics are fixed and tested.

## Request and input handling

The request DTO recognizes `model`, `input`, optional `instructions`, `stream`, `tools`, `tool_choice`, `previous_response_id` and `store` (default true).

Input normalization supports:
- a string, converted to one user message;
- an array of message objects with roles and content;
- `function_call` items, converted into assistant tool-call history;
- `function_call_output` items, converted into tool-result messages;
- selected text, image data URLs and base64 audio blocks.

Remote image URLs are rejected. Only function tools are accepted; other Responses tool types fail closed. Tool arguments are normalized to JSON text when necessary. The prompt is then built from prior response messages, new input and the current `instructions`. It is a textual browser-chat transcript, so it cannot guarantee the exact same role hierarchy, context management, or native function-call semantics as OpenAI's Responses backend.

## Streaming

There are two paths:
- For `stream=true` with no active tools, the router chooses an incremental Responses handler. It emits `response.created`, `response.in_progress`, output-item/content-part events, output-text deltas, done events and `response.completed` on success. It uses monotonically increasing `sequence_number` values.
- For active tool requests or non-incremental cases, the handler completes inference first and then constructs an SSE event sequence from the final result. That has SSE framing but does not stream live deltas during inference.
- The incremental path uses cumulative browser snapshots and emits only appended suffixes; rewritten prefixes cannot be represented.
- It registers the run in `SessionService` for status/events, which is a coupling to the upstream application session subsystem.
- A stream write/disconnect failure is not propagated as an inference cancellation request. The browser run can continue until its own deadline/return.

These event-ordering claims are based on source and unit-test assertions, not a test run in this audit.

## State store and lifecycle

### Current source behavior

- A process-wide `OnceLock<Mutex<StoreHub>>` caches tenant stores.
- Tenant namespace is derived from the API key using a 64-bit FNV-1a hash. This is a namespace label, not the authentication mechanism; request authorization happens separately.
- Store files use the `openai-local-state-v1` format under `config::data_dir()`, with a temporary `.json.tmp` file followed by rename.
- Each tenant store has eviction limits of 256 entries and 64 MiB. The current byte budget calculation serializes `messages` for accounting, not the full stored response object.
- Retrieval, deletion and `input_items` lookup are scoped to the API-key-derived tenant.
- `previous_response_id` retrieves the stored messages; if not found, the request returns 404.
- `store=false` skips storing the newly created response; the hardcoded response object's `store` field does not reflect that.

### Risks / required design choices

1. Choose one intended retention model: memory-only (simpler privacy story) or persistent local state (needs explicit path, permissions, deletion and backup policy). Update code and docs to match.
2. If persistence remains, test atomic replacement, corruption recovery, permissions, restart retrieval, eviction order, full-byte accounting and disk-full/permission failures. Current persistence helper ignores several filesystem/serialization errors, so the caller cannot know whether a response was durably saved.
3. Clarify whether `DELETE /v1/responses/{id}` must remove only the response or its related conversation lineage; test previous-response chains after deletion.
4. Ensure response JSON truthfully reports `store`, `previous_response_id`, `tools`, `tool_choice`, `temperature`, `top_p`, `usage` and other fields. A schema-shaped field set is not evidence that the corresponding feature is implemented.
5. Document the tenant isolation boundary. Hash-derived directory names are not a substitute for file permissions or a secure storage policy.
6. Decide whether `input_items` is a required compatibility endpoint or an upstream-specific extension before carrying it forward.

## Endpoints found in routing

| Endpoint | Source behavior | Audit stance |
|---|---|---|
| `POST /v1/responses` | buffered completion, optional post-completion SSE, optional storage | Partial support; resolve `store` mismatch and input/field policy |
| `POST /v1/responses` with text `stream=true` | incremental SSE via session service | Test event contract, disconnect and failure paths |
| `GET /v1/responses/{id}` | retrieve stored response, tenant-scoped | Depends on storage policy |
| `GET /v1/responses/{id}/input_items` | list normalized stored input/history | Non-baseline endpoint; test shape and retention |
| `DELETE /v1/responses/{id}` | delete one stored ID | Test persistence, chains and response after deletion |

## Required tests before enabling this API

- `store=true` and `store=false` response-body truthfulness for both buffered and streaming responses.
- Restart the process and verify retrieval according to the chosen retention policy.
- Retrieval, deletion and `previous_response_id` with valid, unknown, deleted and cross-tenant IDs.
- Corrupt/missing/unwritable store files; atomic write failure and eviction limits.
- Input string, message array, function call/output, selected image/audio parts and remote URL rejection.
- Response object schema and all exposed field values; no false claims of tool/usage/temperature semantics.
- Exact event ordering, monotonic sequence numbers, completion/failure event and client disconnect.
- OpenAI SDK smoke tests against mock inference, then separately labeled live browser tests.

## Verification status

- **Source-verified:** DTO fields, input normalization path, storage file paths, retrieval/deletion routes, hardcoded response `store` value and incremental/buffered paths.
- **Not verified:** runtime persistence behavior in a fresh process, SDK acceptance, test execution or live Brain behavior. Documentation and source conflict on persistence; source currently indicates disk persistence.
