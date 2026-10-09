# Upstream audit — inference invocation, tool semantics and streaming

- **Audit date:** 2026-10-09
- **Upstream revision:** `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22`
- **Scope:** API handler → prompt/tool normalization → inference adapter → browser turn → final/SSE response.
- **Method:** source inspection of the pinned upstream revision. This is not a local test run or live provider smoke.

## Conclusion

The most valuable reusable contract is `BrowserInferenceRequest` → `BrowserInferenceResponse` and the functions `complete_with_attachments` / `complete_streaming_with_attachments`. It separates API formats from one browser model turn, validates attachments and tool schemas, strips provider UI chrome, and parses a separate `WEBAGENT_INFERENCE/1` tool envelope. The API bridge then adapts that result to OpenAI/Anthropic/Responses wire formats.

Keep the two protocol layers distinct:
- `WEBAGENT_INFERENCE/1` is an internal envelope used to request and validate function calls from the web-chat model.
- `webagent/1` is the separate WebAgent task/action protocol and must not be parsed or executed by this inference service.

## Invocation and locking

### Per-Brain serialization

`api_bridge::inference` keeps a process-wide map of mutexes keyed by `brain.to_ascii_lowercase()`. The map is initialized with `OnceLock`; each Brain gets an `Arc<Mutex<()>>`. A request locks only the selected Brain, so:
- requests for the same mutable web-chat session cannot interleave;
- different Brains can execute concurrently;
- a poisoned lock is recovered via `into_inner` rather than permanently disabling that Brain.

This is a correctness requirement, not merely a throughput optimization. The new application should retain per-Brain single-flight behavior even if its HTTP layer is async. Do not hold a global registry lock while waiting for a model response.

### Blocking path

1. The handler authorizes the request, rejects selected unsupported OpenAI fields, deserializes the request, resolves the model ID, normalizes messages and attachments, validates tools and `tool_choice`.
2. It calls `run_task_blocking` with the normalized prompt, attachments, tools and choice.
3. The inference adapter resolves `auto` to a concrete Brain when needed, applies attachment-aware timeout logic, locks that Brain and calls `browser_inference::complete_with_attachments`.
4. The browser-inference layer validates the request, builds the model-facing prompt/tool instructions, invokes the relay, normalizes the final text and validates/parses any function-call envelope.
5. The handler serializes the normalized answer as an OpenAI completion or returns a provider-shaped error.

The test-only `fake_reply` field bypasses real browser inference; tests using it are protocol tests, not evidence of live Brain compatibility.

### Tool semantics

- Function tools are described to the web-chat model as part of the prompt. The browser model is expected to return a `WEBAGENT_INFERENCE/1` envelope, which is parsed and validated against the supplied tool list.
- Unknown tools, duplicate IDs, invalid forced-tool choices and a text-only answer when a tool is required are intended to fail closed.
- Tool definitions are compacted to fit a 64 KiB serialized schema budget; descriptions can be shortened while names and parameter shapes are retained.
- The bridge returns `tool_calls` to the client. It does not execute the tool and does not accept tool execution as a browser-side command. The client sends the tool result in a subsequent request.
- Text streaming is deliberately not used for active tool-call generation, to avoid exposing a partial machine envelope as user-visible text. Tool streams are buffered until validated.

For the new app, tool support should be declared as a per-Brain capability and tested with a full client round-trip. The existence of tool-call DTOs alone does not establish that a particular web-chat model will produce reliable envelopes.

## Streaming behavior traced

### Chat Completions

The router chooses an incremental handler only when `stream=true` and either the `tools` list is empty or `tool_choice="none"`. The handler:
- sends SSE headers and an initial assistant-role chunk;
- receives cumulative text snapshots from browser inference;
- normalizes each snapshot and emits only the suffix when the new text starts with the last emitted snapshot;
- sends SSE comment keepalives during quiet periods;
- emits a finish-reason chunk and `data: [DONE]` on normal completion.

The incremental inference function is called with no tools, which matches the routing condition: if tools are explicitly disabled, tool definitions should not be used to solicit a tool call. Requests with active tools take the buffered path.

### Important limitations to preserve or improve

1. **Snapshot-diff assumption:** the stream handler assumes each later browser snapshot is a prefix-preserving extension of the previous one. If the web UI revises earlier text, `strip_prefix` cannot express that correction; the changed prefix is not retracted. A new implementation should document this limitation or implement a deliberate snapshot-to-delta policy with tests.
2. **Errors after SSE headers:** once the handler has sent status 200 and SSE headers, it cannot return an ordinary HTTP 4xx/5xx. The inspected error path emits a chunk with `finish_reason="error"` and an `error` object, then returns without the normal finish chunk/`[DONE]`. This needs a compatibility test against intended SDKs; don't silently label all post-header failures fully OpenAI-compatible.
3. **Cancellation:** no request abort token is passed down to `BrowserInferenceRequest`/relay. If the client disconnects, the callback stops attempting further writes, but the browser inference can continue until the relay's timeout/return. A client disconnect is therefore not proof that work was cancelled.
4. **Timeout layers:** the socket read/write timeout is 30 seconds; it is not the inference deadline. Browser readiness and response waiting use separate timeout resolution. New code should separate body-read timeout, inference deadline, stream idle/keepalive and cancellation.
5. **Usage accounting:** the completion response reports zero token counts because the browser UI does not expose actual token usage. This is an explicit placeholder, not measured usage.
6. **Prompt/response mismatch:** browser model output is normalized and can include model-specific UI cleanup; it is not byte-for-byte output from a provider API. Error banners and UI chrome are filtered. That is correct for a web-chat adapter, but needs capability and error semantics tests.
7. **Tool-choice semantics:** mapping API tool modes to prompt instructions is inherently probabilistic because the underlying web-chat is not a native tool API. The bridge can validate returned calls but cannot force the web model to comply in the same way as a provider-side structured tool API.

## Model resolution and AutoRouter

- Model IDs are normalized through the upstream catalog. The documented public IDs are `webagent/<brain>`; aliases include the configured default and `webagent/auto`.
- The catalog reads the configured Brain registry but the AutoRouter preference lists and modality declarations are code-defined. The route classifier uses request attachments, tool presence and keyword heuristics for coding/current research.
- Auto-routing chooses a real Brain before the browser turn, records the decision in logs, and uses circuit-breaker state when selecting available candidates.
- The input/output modality declarations are based on upstream smoke evidence and intentionally conservative for some Brains. They are not a general dynamic provider capability API.
- For this repository, prefer explicit configured model IDs and a data-driven Brain capability registry. Keep heuristic AutoRouter optional; it should never silently send a required image/audio/tool request to a Brain that lacks that capability.

## Error mapping and fail-closed behavior

- Invalid JSON, unsupported fields/content, invalid tool schemas and unknown model IDs are rejected before a browser turn.
- Unknown model IDs are mapped to 404/model-not-found rather than being treated as a generic malformed request.
- Browser/provider inference failures map to provider/server errors in buffered paths.
- Unsupported content types, external media URLs and unrecognized content blocks are rejected instead of silently ignored or fetched.
- Streaming errors after response start require an in-band error policy, since HTTP status is already committed.

These behaviors should be ported as tests before implementation, not by copying helper functions wholesale.

## Test coverage visible in source

The API test module is organized around prompt/content, tool protocols, auth/error shapes, route/streaming behavior, model catalog, store lifecycle, HTTP transport and SDK black-box tests. The source includes tests for:
- plain and block-text message normalization;
- image/audio data URL decoding into attachments;
- rejection of unsupported fields and content blocks;
- tool-choice validation and invalid envelopes;
- auth headers and provider-shaped errors;
- route selection for buffered versus incremental requests;
- SSE sequence numbering/header contracts;
- store tenant isolation and lifecycle;
- request parsing limits/HTTP errors.

This audit did not execute those tests or independently confirm all live-smoke statements in the docs. A clean-checkout test run remains a required gate.

## Recommended contract for the new app

Define an internal normalized request with explicit fields for model ID, messages, stream intent, tools/tool choice, attachments, deadline and cancellation. Define a normalized result/event stream that distinguishes text delta, validated tool call, usage-unavailable, terminal success and terminal failure. Keep OpenAI DTOs and SSE framing in the HTTP adapter. A Brain adapter must not know about HTTP headers, OpenAI JSON objects or agent tool execution.

Required acceptance tests before claiming compatibility:
- buffered text completion and malformed request;
- incremental text completion with normal terminal `[DONE]`;
- disconnect during a long-running turn and verified cancellation policy;
- provider/runtime failure before and after SSE headers;
- cumulative snapshot update, identical snapshot and rewritten-prefix behavior;
- unknown/duplicate/invalid tool call, required-tool mode and tool-result follow-up;
- unsupported parameter/content rejection;
- model not found and auth failure;
- same-Brain concurrency serialization and different-Brain concurrency;
- per-Brain capability checks for image/audio/tool requests.

## Verification status

- **Source-verified:** request/response flow, per-Brain locking, tool-envelope separation, buffered versus incremental route policy, stream delta construction and error behavior.
- **Not verified:** test execution, SDK behavior on the error path, cancellation under client disconnect, or live provider reliability.
