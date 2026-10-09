# Integration strategy — 2026-10-09

## Binding architecture decision

Build the product's own OpenAI-compatible HTTP API and its own internal Brain interface. **Do not use WebAgent's public API bridge or `webagent::api_bridge::serve` as the product core.** That would miss the project's purpose: expose browser chat interfaces through an API implemented and controlled by this repository.

WebAgent is an upstream source to study, not an automatically approved dependency. No source component may be copied, adapted, or added as a dependency until it has a component-level evaluation record. See [the mandatory evaluation gate](UPSTREAM_COMPONENT_EVALUATION_POLICY.md).

## Product boundary

```text
OpenAI-compatible client
  -> this repository's HTTP/API layer
      -> request validation and normalization
      -> model/Brain registry and capability declarations
      -> internal BrainBackend contract
          -> narrowly scoped browser-chat adapter/runtime
              -> authenticated browser chat interface
```

The product's API implementation owns routes, request/response semantics, authentication, error mapping, SSE, cancellation, model IDs, capability claims, and tests. The Brain adapter owns only the browser-side work needed to submit a prompt and obtain a reply. Keep agent orchestration, shell execution, autonomous loops, local tool runners, and the `webagent/1` action protocol out of the inference path.

## Required order of work

1. **Define evaluation candidates.** Use the existing source audits to identify exact functions/modules that might be necessary for browser startup, session/profile handling, input submission, reply extraction, streaming, and cleanup.
2. **Evaluate each candidate individually.** Record source revision/path/symbol, call graph, dependencies, side effects, tests, limitations, licensing/security, alternatives, decision, and verification plan. No copy/paste or dependency adoption before this record is reviewed.
3. **Define our own BrainBackend contract.** Keep it independent of WebAgent types and of OpenAI wire DTOs. Include normalized request/result/events, capabilities, readiness/failure categories, timeout and cancellation behavior.
4. **Implement the smallest independent API slice.** Start with `GET /v1/models` and `POST /v1/chat/completions` for text, first buffered and then SSE. Reject unsupported request fields explicitly rather than silently ignoring them.
5. **Implement only approved runtime pieces.** Use the component decisions to determine whether a narrow source adaptation, a small dependency for a specific non-API component, or an independent implementation is justified. Do not pull in the existing API bridge as a shortcut.
6. **Verify in layers.** Unit/fixture tests for the API contract; isolated adapter tests; then live browser tests using a disposable profile. Record exact commands, OS/toolchain, upstream commit, redacted logs, and results in `docs/proofs/`.
7. **Add client compatibility and UI later.** A successful smoke test with one client does not establish compatibility with others. The operator/test UI must call the same API, not implement a second inference path.

## Security and isolation requirements

- Bind to loopback by default; require authentication and explicit configuration for any broader exposure.
- Use an app-specific browser profile/data root by default. Never silently reuse another WebAgent installation's profile.
- Treat browser profiles, cookies, Local Storage, IndexedDB, API tokens, prompts, and replies as sensitive.
- Do not log prompts/replies or credentials by default.
- Distinguish login-required, rate-limited, timed-out, crashed, and unknown states.
- Do not claim cancellation works until disconnect and runtime cancellation behavior are tested.

## Initial API scope

Required first slice:
- `GET /health`
- authenticated `GET /v1/models`
- authenticated `POST /v1/chat/completions` with buffered text response
- text SSE streaming with explicit error and terminal-event semantics

Defer Responses API, Anthropic routes, images/audio, tool calls, persistence, and UI until the initial contract and browser path pass. Advertise only capabilities that have been verified per Brain.

## Stop/go criteria

**Go** only when component-level records justify the chosen browser/runtime pieces, the independent Brain contract is stable, and the minimal API/browser path passes clean-checkout, security, and live-browser tests.

**Stop and redesign** if a required upstream component cannot be isolated safely, carries unacceptable coupling, or cannot be tested reliably. Do not resolve such a failure by adopting WebAgent's public API bridge or copying a broad subsystem without a new evaluation.

## Existing evidence

The existing source audits remain useful for discovery:
- [Upstream module inventory and call graph](UPSTREAM_AUDIT_2026-10-09.md)
- [Brain runtime lifecycle](BRAIN_RUNTIME_AUDIT_2026-10-09.md)
- [Inference, locking, and streaming](INFERENCE_STREAMING_AUDIT_2026-10-09.md)
- [Chat Completions compatibility](CHAT_COMPLETIONS_AUDIT_2026-10-09.md)
- [Responses API and persistence](RESPONSES_API_AUDIT_2026-10-09.md)
- [Media capabilities](MEDIA_CAPABILITY_AUDIT_2026-10-09.md)
- [File-level reuse matrix](UPSTREAM_REUSE_DECISION_2026-10-09.md)

Those are source-level audits, not proof of a successful build or live browser integration. No application build, live browser turn, or client compatibility test is claimed by this document.
