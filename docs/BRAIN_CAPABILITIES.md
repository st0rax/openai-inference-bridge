# Per-Brain capability declarations

Status: P-022 design proposal, 2026-10-09. No capability is yet verified by a live adapter test.

## Principles

- Capabilities are declared per configured Brain, not inferred from its provider name or URL.
- A declared capability is a product claim and must point to project-local test evidence.
- Runtime readiness (logged in, challenged, rate-limited, browser alive) is separate from capability. A capable Brain can be temporarily unavailable.
- Unknown or untested capability is treated as unsupported. Do not silently fall back to another Brain.
- The API layer validates requests against capabilities before dispatch. The browser adapter does not parse OpenAI wire DTOs.

## Initial capability model

The project-owned internal record should represent:

- **Text input:** can the adapter submit the normalized prompt to this web-chat?
- **Text output:** can it reliably extract the final assistant answer?
- **Text streaming:** one of:
  - `BufferedOnly`: only the final answer is reliable.
  - `VerifiedAppendOnly`: live tests prove multiple changing snapshots and every emitted update is append-only for the tested Brain/site version.
- **Tools:** unsupported in the initial milestone. Prompt-based tool envelopes are not equivalent to native tool support.
- **Images/audio and generated media:** unsupported in the initial milestone.

The HTTP API may accept `stream: true` at the protocol layer, but it must not advertise true incremental streaming for a `BufferedOnly` Brain. Until a tested mapping exists, a request that requires unsupported behavior is rejected explicitly rather than silently degraded.

## Evidence record

Every capability set must identify:
- Brain ID and adapter version;
- exact test/proof path;
- date and target OS/browser runtime;
- status: `verified`, `partial`, or `unverified`;
- tested behavior and known limitations.

Only `verified` evidence may enable a capability by default. `partial`/ `unverified` means disabled. A test passing for one Brain must not enable the same capability for every Brain.

For streaming, evidence must show more than one distinct response snapshot during a single live generation and confirm that each snapshot extends the prior text without rewriting it. A single final response delivered through SSE is buffered delivery, not verified incremental streaming.

## Initial defaults

For a new/unverified Brain:
- text input: disabled until submission is tested;
- text output: disabled until extraction/completion detection is tested;
- text streaming: `BufferedOnly`;
- tools, image input, audio input and generated media: disabled.

The first supported Brain can be enabled only after a disposable-profile test proves prompt submission, final-answer extraction, and failure handling. Capability declarations must remain conservative even when the provider's website offers more features than this adapter supports.

## Validation and API mapping

- Unknown capability names and invalid combinations fail configuration validation.
- `VerifiedAppendOnly` requires a matching evidence record; it cannot be set by a request.
- Capability checks occur before browser work starts, so unsupported requests do not navigate or consume a model turn.
- The registry/model list stays fast and does not launch a browser to discover capabilities.
- The API layer maps unsupported request features to an explicit client error; it does not silently drop tools, images or other fields.

## Required tests

1. Unverified capabilities default to disabled.
2. One Brain's evidence cannot enable capabilities for another Brain.
3. Streaming defaults to `BufferedOnly`.
4. Append-only streaming cannot be enabled without evidence.
5. Unsupported tools/media are rejected before browser startup.
6. Temporary login/challenge/rate-limit state does not rewrite the static capability declaration.
7. API model listing does not imply current readiness.
8. Capability changes are traceable to a code/config change and tests.

## Deferred

Provider model selection, tool calling, attachments, audio/image processing, generated media, dynamic capability discovery and automatic routing are outside the initial text-only milestone.
