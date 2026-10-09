# Upstream reuse decision

**Binding decision:** do not use WebAgent's public API bridge as the core or integration boundary. Implement this repository's OpenAI-compatible HTTP API and internal Brain contract independently. Every candidate upstream component must be evaluated individually before code reuse or dependency adoption.

The source revision reviewed by the discovery audits is `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22` (package version `0.11.3`). The upstream repository declares MIT; transitive license closure has not been audited.

## What the existing audit means

The existing file-level matrix and runtime/API audits are **discovery evidence only**. They identify useful behaviors, call graphs, dependencies, and risks. They do not grant blanket approval to copy files, symbols, or modules, nor do they validate a public library integration.

The previous recommendation to prototype with `webagent::api_bridge::serve(BridgeConfig)` is superseded. That interface already exposes an API bridge and would delegate a core part of this project's purpose to upstream code.

## Required decision for each candidate

Before implementation, record exact source revision/path/symbol, purpose, call graph and dependencies, state/config/profile assumptions, side effects, relevant tests and limitations, security and licensing considerations, alternatives, explicit decision, and verification plan. Follow [the mandatory component-evaluation gate](UPSTREAM_COMPONENT_EVALUATION_POLICY.md).

Possible outcomes:
- **Adapt narrowly:** only the necessary, understood behavior is ported and covered by tests.
- **Reimplement:** upstream coupling, semantics, or maintenance cost outweigh reuse value.
- **Extract a bounded component:** only when its boundary and dependencies can be proven narrow.
- **Reject:** behavior is out of scope, unsafe, redundant, or insufficiently understood.

Do not use WebAgent's public API bridge as an alternative outcome. If the browser runtime cannot be safely separated from unrelated subsystems, document that and design a project-owned adapter/runtime boundary rather than copying the whole subsystem.

## Source areas to evaluate, not pre-approved components

| Source area | Candidate value to investigate | Evaluation concerns |
|---|---|---|
| `src/brain.rs` | Lifecycle concepts for an internal Brain contract | Coupling to upstream types and assumptions; design our own types first |
| `src/browser_inference.rs` | Normalized browser request/reply flow | Relay, observer, diagnostics, attachments, streaming and internal tool envelope |
| `src/browser/mod.rs`, `src/browser/backend.rs` | Concrete browser-chat interactions | Selectors, UI automation, WebView, profile and config dependencies |
| `src/browser_pool.rs`, `src/webview_runtime.rs`, `src/page_driver.rs` | Browser startup, lifetime and cleanup | Platform coupling, thread/event-loop assumptions, global state and UI dependencies |
| `src/config/brains.rs`, `src/config/profiles.rs`, `src/session.rs` | Brain definitions and session/profile handling | Authenticated secrets, persistence, path assumptions and isolation |
| `src/relay.rs`, `src/observer.rs`, timeout/circuit-breaker modules | Retry, timeout and failure classification concepts | Scoring/benchmark coupling, hidden retries, diagnostics and cancellation semantics |
| `src/api_bridge/**` | Reference material for protocol edge cases and tests only | Do not adopt its public server or copy it wholesale; implement this project's own HTTP/API layer |

## Product-owned architecture

```text
OpenAI-compatible client
  -> this repository's HTTP/API implementation
      -> normalized request/event types
      -> model registry and capability declarations
      -> project-owned BrainBackend interface
          -> reviewed narrow browser-chat adapter/runtime
```

Keep agent controllers, TUI, shell executor, autonomous loops, local tool execution and `webagent/1` out of the inference path.

## Current evidence status

- Source-level discovery audits: complete.
- Initial browser-text component evaluation: complete; see [report](COMPONENT_EVALUATION_2026-10-09.md).
- Approved WebAgent source reuse: **none**.
- Application code: not started.
- Clean build, live browser turn, and client compatibility: not run from this repository.
- Transitive dependency license review: outstanding.
