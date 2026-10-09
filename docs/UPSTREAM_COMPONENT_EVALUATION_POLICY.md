# Upstream component evaluation gate

## Binding decision

The product must implement its own OpenAI-compatible HTTP/API layer and its own internal Brain abstraction. Do **not** use WebAgent's public API bridge (including `webagent::api_bridge::serve`) as the product's core or as the integration boundary. Doing so would delegate the central bridge behavior to an existing API implementation rather than build this project.

No WebAgent source code may be copied, adapted, or added as a dependency merely because an upstream audit found it useful. Every proposed source component must be evaluated individually before implementation begins.

This policy concerns WebAgent's public Rust API/library boundary. It does not prohibit the product's own OpenAI-compatible API, nor does it prohibit studying upstream code to understand browser interaction.

## Required evaluation record for each candidate

Before writing or porting code, add a record to the component evaluation report containing:

1. **Exact source:** upstream commit SHA, path, symbol/function, and relevant line range.
2. **Purpose:** the specific behavior needed by this project and why it belongs in scope.
3. **Call graph and dependencies:** direct/transitive modules, global state, configuration, profile/session assumptions, platform dependencies, and side effects.
4. **Behavioral evidence:** relevant upstream tests, limitations, failure modes, and any behavior not verified by source inspection.
5. **Options considered:** reimplement independently, adapt a minimal algorithm, extract a narrow component, or reject it. Do not treat the upstream public API bridge as an option for the core.
6. **Security and licensing:** authentication/session handling, filesystem/network effects, secrets/logging, license obligations, and relevant transitive dependencies.
7. **Decision and rationale:** accept, adapt, reimplement, or reject, with a bounded scope and explicit risks.
8. **Verification plan:** tests required to demonstrate the chosen behavior in this repository.

A component is not approved until the record is reviewed and its decision is explicit. A general repository audit or a successful upstream test suite does not approve every file.

## Implementation rules

- Start from this project's contracts and requirements, not from a source file to copy.
- Build the OpenAI HTTP routes, DTO validation, error mapping, SSE semantics, model registry, and internal Brain interface here.
- Keep the Brain adapter narrow: browser/session lifecycle and reliable extraction of user-visible replies only.
- Evaluate candidate browser/runtime components individually; prefer the smallest justified extraction. If the required runtime is too coupled to extract safely, document that finding and design a narrow replacement boundary rather than copying a subsystem blindly.
- Do not include the upstream agent controller, TUI, shell executor, autonomous loops, local tool runner, or `webagent/1` action protocol in the inference path.
- Preserve required notices for any adapted/copied code. Audit license obligations before distribution.
- Record source commit pins and provenance for every accepted component.
- Keep unverified findings labeled as such. No build, live-browser, or compatibility result may be claimed without running it.

## Gate before runtime implementation

The next upstream-related task is **component-by-component evaluation**, not a dependency/build spike. No runtime code implementation or upstream code reuse starts until the evaluation records identify the minimum viable components and the project-specific Brain contract is defined.

## Current status

The repository-wide source audits and initial reuse matrix exist, but they were written under an earlier recommendation to use WebAgent's public API bridge. That recommendation is superseded. Existing audit documents remain useful as evidence; their integration recommendation must not be treated as approved. The individual component approval gate is still outstanding.
