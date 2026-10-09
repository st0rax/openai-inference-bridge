# Integration and migration strategy — 2026-10-09

## Decision

Use the public `webagent` library as a pinned runtime dependency for the first working application. Keep the new executable, configuration contract, packaging and operator/test UI in this repository. Do not copy the API bridge or browser runtime modules at the start.

Reviewed upstream pin: `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22` (package `webagent` version `0.11.3`). This is a recommendation, not yet a verified dependency build.

## Why this is the lowest-risk path

- The API bridge already has a public `webagent::api_bridge::serve(BridgeConfig)` entry point.
- The runtime's hard parts—browser selectors, profile handling, login detection, upload paths, WebView lifecycle, shared tabs and failure recovery—are coupled across many upstream modules.
- Copying only the API module would duplicate protocol logic without bringing a runnable browser runtime.
- A pinned dependency keeps the upstream revision explicit and avoids a floating-branch behavior change.
- The new process is independent of client harnesses: Pi/OpenCode/OpenClaw/ZeroClaw consume the HTTP API; none is embedded or wrapped.

## Phase A — dependency/build spike (first implementation gate)

1. Create a minimal Rust binary package named `openai-inference-bridge` with a direct Git dependency on `webagent` pinned to the audited revision.
2. Verify the public library types compile from a clean checkout:
   - `webagent::api_bridge::BridgeConfig`
   - `webagent::api_bridge::serve`
3. Build on the primary target OS with the WebView feature enabled. Record required OS libraries/toolchain and clean-checkout commands. Do not assume the dependency is lightweight: upstream exposes many modules from its library root and uses a platform-sensitive WebView stack.
4. Implement a thin startup/config wrapper only. Do not invoke the upstream CLI/TUI, AgentController, shell executor or local tool runner.
5. Keep `BridgeConfig.fake_reply = None` in production. Use fake responses only in isolated tests.
6. Pin the resolved dependency in `Cargo.lock` and record the upstream revision in build metadata or diagnostics.

**Build spike passes only if** the wrapper builds on the target OS, the API listener starts, `/health` responds, `/v1/models` is authenticated, and one buffered + one streaming text turn work against a disposable authenticated Brain profile. Unit/mock success alone does not prove the live browser path.

## Phase B — process configuration and security

### Configuration

Recommended initial configuration:
- bind: `127.0.0.1` only;
- port: `8788`, unless explicitly overridden;
- default Brain: `chatgpt`, configurable by ID;
- timeout: upstream dynamic default unless explicitly set;
- headless/off-screen WebView: explicit option, with a tested default for the target OS;
- API token: `OIB_API_TOKEN` (or an explicitly documented compatibility fallback to `WEBAGENT_API_KEY`); fail startup when absent or too weak;
- no secrets in CLI args, config files, repository, logs or crash diagnostics.

The wrapper should reject non-loopback bind addresses even before calling upstream. The upstream server also enforces loopback, but a second guard documents and tests this app's security boundary.

### Profile/data isolation

Set `WEBAGENT_ROOT` to an application-specific user-data directory **before any upstream config/runtime calls**. The upstream default root is named `webagent` and may overlap an existing WebAgent installation. Defaulting to a separate directory avoids accidental sharing of authenticated sessions.

- Windows: prefer an app-specific directory under `%LOCALAPPDATA%`.
- Linux: prefer `$XDG_DATA_HOME/openai-inference-bridge`, falling back to the user's local data directory.
- macOS: use an app-specific user Application Support directory if macOS is supported.
- Sharing an existing profile must be an explicit opt-in, with a warning that cookies, Local Storage and IndexedDB are credentials.

Do not store profile data inside the checkout, build directory, Docker image, release archive or CI cache.

### API surface

The direct `api_bridge::serve` entry point exposes the routes implemented by the upstream bridge; it does not provide a per-route allowlist and does not itself host the operator UI. Start by documenting the actual route/capability matrix. If the application must expose only a selected subset, add a deliberate route-policy hook or a reviewed local gateway rather than claiming the current entry point disables routes.

## Phase C — API contract acceptance

Initial required endpoints:
- `GET /health`
- `GET /v1/models`
- `POST /v1/chat/completions` buffered text
- `POST /v1/chat/completions` text SSE

Before client integration, verify:
- valid and invalid API token;
- unknown model ID;
- malformed JSON and unsupported fields;
- missing/unsupported messages and roles;
- correct terminal SSE event and `[DONE]`;
- provider failure before headers and error after headers;
- client disconnect and documented cancellation behavior;
- same-Brain requests serialize; different Brains can run independently;
- login-required, rate-limit, timeout and browser crash are distinguishable;
- no prompt, response, cookie, token or profile path leaks in ordinary logs.

OpenAI SDK/harness tests should be recorded individually. Do not generalize a passing Pi smoke to OpenCode/OpenClaw/ZeroClaw or to every Brain.

## Phase D — operator/test UI

Only after the API spike passes, implement a small UI that calls the same API:
- list configured model IDs and declared capabilities;
- show readiness as `unknown`, `ready`, `login required`, `temporarily unavailable` or `error` without overclaiming;
- send a test prompt and display streaming output;
- expose redacted diagnostics and exact failure categories;
- never display/store the API token or browser profile contents.

The UI is not a second inference implementation and must not contain autonomous agent loops or tool execution.

## Phase E — longer-term source extraction (only if needed)

Revisit source-level independence if the pinned dependency is too large, too hard to package, or changes too frequently. Before extracting:
1. Propose a dedicated upstream runtime crate or a documented minimal dependency boundary.
2. Move normalized request/result types and Brain lifecycle contracts into that boundary.
3. Replace dependencies on benchmark scoring, capability proof, UI/window layout and global app config with injected interfaces.
4. Separate profile/session persistence policy from the browser runtime.
5. Provide Windows and Linux lifecycle tests using disposable profiles.
6. Extract API wire/normalization modules only after the runtime boundary is stable.
7. Preserve upstream MIT notices and audit transitive licenses.
8. Run compatibility fixtures and live smoke tests before switching the application from the pinned dependency.

Do not copy the full `src/api_bridge/` or `src/browser/` tree into this repo as a shortcut.

## Stop/go criteria

**Go with the dependency approach** if clean build, target OS packaging, isolated profile root, API smoke and client compatibility tests pass and the dependency footprint is acceptable.

**Stop and redesign** if the library cannot be cleanly consumed, default paths cannot be isolated, target OS WebView support is unavailable, or the runtime pulls unacceptable application behavior into the new process. The alternative is a scoped extraction plan—not a blind source copy.

## Evidence and handover

Source audit documents:
- `UPSTREAM_AUDIT_2026-10-09.md`
- `BRAIN_RUNTIME_AUDIT_2026-10-09.md`
- `INFERENCE_STREAMING_AUDIT_2026-10-09.md`
- `CHAT_COMPLETIONS_AUDIT_2026-10-09.md`
- `RESPONSES_API_AUDIT_2026-10-09.md`
- `MEDIA_CAPABILITY_AUDIT_2026-10-09.md`
- `UPSTREAM_REUSE_DECISION_2026-10-09.md`

**Not yet done:** Cargo dependency build, upstream test execution, live browser turn, SDK integration test, packaging or UI implementation.
