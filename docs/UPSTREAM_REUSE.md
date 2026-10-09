# Upstream audit and reuse decisions

Status: initial hypotheses only; no source files copied.

## Repositories

- `st0rax/webagent-rs` — primary technical reference for the existing API bridge and browser-based Brain runtime.
- `st0rax/dummy-bazaar` — workflow/template reference only.

## Known source areas to inspect

From the current `webagent-rs` architecture notes and `src/api_bridge.rs`:

- `src/api_bridge.rs`
- `src/api_bridge/boundary.rs`
- `src/api_bridge/catalog.rs`
- `src/api_bridge/content.rs`
- `src/api_bridge/inference.rs`
- `src/api_bridge/provider_handlers.rs`
- `src/api_bridge/response_protocol.rs`
- `src/api_bridge/routing.rs`
- `src/api_bridge/store.rs`
- `src/api_bridge/transport.rs`
- `src/api_bridge/wire.rs`
- `src/brain.rs`
- browser backend/session lifecycle modules and runtime wiring

These paths and their actual contents must be checked against the current upstream tree before a copy or extraction. The architecture document warns that source files without module wiring are not production-wired; verify both implementation and call sites.

## Initial decision matrix

| Area | Initial stance | Reason / required proof |
|---|---|---|
| HTTP parsing and SSE wire helpers | Adapt or reuse selectively | Check coupling to root types and exact protocol behavior |
| Auth/error boundary | Adapt selectively | Recheck security behavior and config contract |
| Model catalog/selection | Adapt | Separate Brain IDs from WebAgent CLI/profile semantics |
| Prompt/tool content normalization | Audit first | Keep only API-required normalization; do not inherit unsupported claims |
| Browser inference invocation | Reuse behind a narrow adapter if feasible | Must avoid AgentController, shell executor and webagent/1 action protocol |
| BrainBackend/session lifecycle | Likely reuse/adapt | Determine minimal dependency closure and WebView ownership constraints |
| Response storage/retrieval | Optional; decide by endpoint scope | Avoid persistence unless required and documented |
| Anthropic protocol handlers | Defer unless product scope requires them | OpenAI-compatible API is the first boundary |
| Multimodal handling | Evidence-based, likely staged | Current architecture notes mark media handling as deferred in some areas |
| UI, CLI, TUI, benchmarks, workers, autonomous controller | Exclude initially | Outside the standalone inference-bridge boundary |
| `dummy-bazaar` files | Recreate/adapt workflow, not copy blindly | Remove template placeholders and project-specific assumptions |

## Required outcome

`P-017` must replace these hypotheses with a file-level table containing: path, responsibility, call sites, dependency closure, tests, license/attribution implications, decision (`reuse`, `adapt`, `rewrite`, `exclude`), and rationale. No production code should be copied before that decision is reviewed.
