# Browser Brain Adapter

The project-owned adapter lives in `src/brain_backend.rs`. It has no WebAgent types or WebAgent dependency.

## Boundary

- `BrainBackend` defines start, readiness, one-turn inference with snapshot events, and shutdown.
- `BrowserBrainBackend<D>` implements the lifecycle and turn semantics over the project-owned `BrowserPageDriver` trait.
- A concrete driver must own the actual WebView/page objects and honor the same absolute deadline for each operation.
- The adapter accepts only enabled registry entries with adapter kind `chatgpt-web`, uses the registry-resolved profile path, and never accepts profile paths or URLs from an API request.

## Turn behavior

1. Start the configured profile/page before inference.
2. Check the shared deadline and cancellation state.
3. Require a positive `Ready` result; distinguish login-required, challenge, rate-limit, unknown, and failed readiness.
4. Submit the normalized prompt once; no hidden retry loop.
5. Emit full assistant-text snapshots when observed. Snapshots may repeat or be revised; they are not promised to be deltas.
6. Treat the final non-empty complete snapshot as authoritative. Empty completion is a typed failure.
7. Shut down owned resources explicitly. A failed shutdown poisons the backend against reuse.

The adapter does not log prompts, replies, URLs, cookies, or profile paths. Backend error messages are static, safe diagnostics.

## Verification and limitations

Unit tests use a fake driver to verify snapshot flow, final-result semantics, readiness failures, cancellation, deadlines, and shutdown. These tests do not establish browser correctness.

There is **no concrete WebView driver yet**. P-041 adds the per-Brain worker manager, but no browser is launched and no live provider page is controlled. A platform driver and executable wiring remain outstanding. Until those exist, Chat Completions returns `503 brain_runtime_unavailable` for configured models.
