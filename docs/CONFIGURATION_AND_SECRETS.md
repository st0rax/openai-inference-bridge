# Configuration and Secret-Handling Policy

This document is normative for the bridge's configuration, authentication, and browser-profile boundaries. It distinguishes implemented behavior from requirements for the future HTTP/runtime layer.

## Implemented configuration behavior

- `OIB_BIND` defaults to `127.0.0.1:8788`.
- `OIB_API_TOKEN` is required and must contain at least 32 printable ASCII characters.
- `OIB_ALLOW_REMOTE=1` is required for a non-loopback bind address.
- `OIB_DATA_DIR`, when set, must be absolute. Otherwise the application selects a per-user platform data directory.
- `Config` redacts the token in its `Debug` representation.
- Brain profile paths are rooted under `<data-dir>/profiles/<validated-brain-slug>`. The bridge does not implicitly reuse a WebAgent profile.
- See [configuration reference](CONFIGURATION.md) for variable names and platform paths.

These guarantees currently apply to the configuration module. The HTTP server, request authentication, browser runtime, and profile creation are not yet implemented.

## Secret sources and handling

1. Read the API token from `OIB_API_TOKEN`; do not accept it as a command-line argument or commit it to a config file.
2. Never log, serialize, return, or include the token in errors, panic messages, debug output, telemetry, or diagnostics.
3. Log configuration names and safe state only (for example, bind address and whether authentication is configured); never log secret values.
4. Authentication failures must not echo the supplied Authorization header or token.
5. Keep provider login/session material inside the corresponding Brain profile. Do not copy cookies, browser storage, or authentication state between profiles.
6. Treat profile directories as sensitive user data. Restrict filesystem permissions where the platform supports it; do not expose profile contents through API endpoints or diagnostics.
7. Do not create a profile directory until the Brain slug has passed validation. Reject path traversal, separators, empty slugs, and characters outside the documented slug grammar.

## HTTP authentication requirements

When the HTTP layer is implemented:

- Require `Authorization: Bearer <token>` for all API routes except any explicitly documented liveness endpoint.
- Compare presented and configured tokens using a constant-time comparison where practical.
- Reject missing, malformed, or incorrect credentials before dispatching work to a Brain.
- Do not support query-string tokens, URL credentials, or token values in request bodies.
- Return a generic authentication error without disclosing which token component was wrong.
- Do not persist the API token to disk. The configured environment is the source of truth for the process lifetime.
- Keep error bodies and request logs free of prompts, provider cookies, and authorization headers by default.

The API token currently grants access to the whole local bridge; per-client identities, scopes, and token rotation are not implemented. A future change must document those capabilities rather than imply they already exist.

## Network exposure

Loopback is the default and preferred deployment. `OIB_ALLOW_REMOTE=1` only permits configuration of a non-loopback address; it does **not** provide TLS, firewall protection, or remote-access hardening.

Remote exposure must not be described as secure based on the opt-in flag alone. Before recommending it, the project must provide and test an explicit transport-security and deployment model. Until then, use loopback or a separately secured tunnel, and keep the API token secret.

## Verification requirements

Any change to configuration or authentication must include tests for the relevant behavior:

- default loopback bind and explicit remote opt-in;
- missing, weak, malformed, and valid token handling;
- no token leakage through Debug, error, response, or log representations;
- authentication rejection before Brain dispatch;
- absolute data directory enforcement and platform-specific fallback behavior;
- profile slug validation and isolation between Brains.

Run the repository verification commands recorded in `docs/TASKBOARD.json`: `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked`.

## Current limitations

This policy does not claim that API authentication, HTTP routing, secure remote transport, browser profile permissions, or provider session management are implemented. Those controls remain acceptance criteria for the corresponding runtime tasks.
