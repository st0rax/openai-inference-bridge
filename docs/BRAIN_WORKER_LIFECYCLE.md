# Per-Brain Worker Lifecycle

The initial worker manager lives in `src/brain_manager.rs`. It owns one worker thread and one backend instance per configured enabled Brain.

## Guarantees

- Each worker processes commands sequentially, so turns for the same Brain cannot overlap.
- Different Brain workers run independently and can execute concurrently.
- Browser/backend objects are created by an injected factory and remain owned by their worker thread.
- Start, readiness, inference, and shutdown operations use absolute deadlines.
- Inference snapshot events are forwarded to the caller while the final result is awaited.
- Timeouts, cancellation, and failures that may leave browser state uncertain poison the worker. New requests are refused rather than silently reusing it.
- Explicit shutdown waits only until the supplied deadline. Manager drop attempts best-effort cleanup with a short deadline.

## Important limits

- The manager owns a `BrainBackend` factory. The executable now registers the project-owned `BrowserCdpDriver`, which launches Chromium/Edge through a loopback-only DevTools endpoint.
- A timed-out driver operation that ignores its deadline may continue in a detached worker thread. The manager will not reuse that worker, but OS-level resource reclamation depends on the driver eventually returning.
- This does not provide cross-process locking. A future profile/session layer must ensure that two bridge processes cannot open the same browser profile concurrently.
- The HTTP server constructs a `BrainManager` and routes non-streaming Chat Completions through it. Live provider interaction has not yet been verified; streaming and token accounting remain unsupported.

Tests use fake backends to verify same-Brain serialization, cross-Brain concurrency, timeout poisoning, and shutdown.
