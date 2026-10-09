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



## Brain registry

The initial registry is configured through environment variables. If `OIB_BRAIN_IDS` is unset or empty, the model list is empty.

For each comma-separated canonical ID in `OIB_BRAIN_IDS`, set `OIB_BRAIN_<ID_IN_UPPERCASE>_ENABLED` to `1` or `0`. An enabled Brain also requires:

- `OIB_BRAIN_<ID>_LABEL`: display label; not used as the model ID.
- `OIB_BRAIN_<ID>_URL`: HTTP(S) start URL without embedded credentials.
- `OIB_BRAIN_<ID>_ADAPTER`: currently the supported value is `chatgpt-web`.

Example (PowerShell):

```powershell
$env:OIB_BRAIN_IDS = "chatgpt"
$env:OIB_BRAIN_CHATGPT_ENABLED = "1"
$env:OIB_BRAIN_CHATGPT_LABEL = "ChatGPT"
$env:OIB_BRAIN_CHATGPT_URL = "https://chatgpt.com/"
$env:OIB_BRAIN_CHATGPT_ADAPTER = "chatgpt-web"
```

The model ID is exactly `oib/chatgpt`. IDs must be lowercase ASCII slugs matching `[a-z0-9][a-z0-9-]{0,62}`; duplicate or non-canonical IDs fail startup. Disabled entries need only the ID and enabled flag and are omitted from `GET /v1/models`. Model listing does not launch a browser or check login/readiness. The ChatGPT adapter is a registry identifier only; browser execution is not implemented yet.

See [configuration and secret-handling policy](CONFIGURATION_AND_SECRETS.md) for normative requirements for HTTP authentication, logging, provider profiles, and future remote exposure.
