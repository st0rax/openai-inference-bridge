# Configuration

The bridge configuration is read from environment variables. The server runtime is not yet wired to this configuration module.

| Variable | Default / requirement | Meaning |
| --- | --- | --- |
| `OIB_BIND` | `127.0.0.1:8788` | Socket address to bind. |
| `OIB_API_TOKEN` | Required | Bearer token; at least 32 printable ASCII characters. |
| `OIB_ALLOW_REMOTE` | Unset | Must equal `1` to allow a non-loopback bind address. |
| `OIB_DATA_DIR` | Platform-specific | Absolute application data directory. |

Default data directory:
- Windows: `%LOCALAPPDATA%\OpenAIInferenceBridge`, with a `%USERPROFILE%\AppData\Local\OpenAIInferenceBridge` fallback.
- macOS: `~/Library/Application Support/OpenAIInferenceBridge`.
- Linux and other Unix systems: `$XDG_DATA_HOME/openai-inference-bridge` when XDG data home is absolute; otherwise `~/.local/share/openai-inference-bridge`.

Each Brain's browser profile path is isolated below `profiles/<brain-slug>`. Slugs must start with a lowercase ASCII letter and contain only lowercase ASCII letters, digits, or hyphens. The bridge does not reuse a WebAgent profile by default.

The token is redacted from the configuration's Debug representation. Do not pass secrets as command-line arguments or log their values. A remote bind opt-in is not a substitute for TLS, firewall policy, or a security review; remote serving should remain disabled unless the deployment deliberately supplies those controls.

The configuration is loaded by the initial local HTTP listener. The listener enforces bearer-token authentication, but the API routes still return `501 Not Implemented` and no browser runtime is connected yet.


See [configuration and secret-handling policy](CONFIGURATION_AND_SECRETS.md) for normative requirements for HTTP authentication, logging, provider profiles, and future remote exposure.
