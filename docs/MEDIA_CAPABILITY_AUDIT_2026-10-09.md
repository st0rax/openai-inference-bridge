# Upstream audit — media paths and per-Brain capabilities

- **Audit date:** 2026-10-09
- **Upstream revision:** `st0rax/webagent-rs@a6693dcc8095b3306a11593a216741f8a5c85a22`
- **Method:** source and documented capability-matrix inspection. No live media upload/generation was rerun.

## Summary

Media support is not one generic capability. There are distinct code paths for image/audio input attachments, audio transcription/translation, image generation and speech generation. Each has different transport, UI interaction and evidence requirements. The source is designed to reject unsupported content rather than fetch external URLs or silently pretend that a browser UI can handle every modality.

The new app should model capabilities per Brain and per operation, for example:
- `input.text`
- `input.image`
- `input.audio`
- `output.text`
- `output.image_generation`
- `output.audio_generation`
- `tools.function_calls`

Do not collapse all of these into a single `multimodal=true` flag.

## Input attachment path

### Accepted wire representations observed

- OpenAI Chat Completions: image `image_url` with a base64 `data:image/...` URL; audio `input_audio` with base64 data plus a recognized format.
- OpenAI Responses: selected `input_image` data URLs and `input_audio` base64 parts.
- Anthropic Messages: `image` / `audio` blocks whose source type is `base64`, with a MIME type.
- Remote `http(s)` URLs, file IDs and unsupported block types are rejected; the bridge does not download media on behalf of the client.

The content normalizer decodes the wire data into `BrowserAttachment { kind, file_name, mime_type, data }`, adds a textual attachment marker to the prompt, and passes the bytes to the browser-inference upload path. The browser UI then needs to accept the file, and successful upload must be confirmed through the visible UI/preview path. Parsing a valid data URL is not proof that the Brain can consume the media.

### Limits and validation

- HTTP request envelope limit: 16 MiB, including JSON and base64 overhead.
- Decoded attachment limit: 8 MiB per attachment.
- Maximum: 16 image/audio attachments per request.
- Empty attachment data is rejected.
- The MIME type is checked by the `image/` or `audio/` prefix; the inspected validation does not sniff file signatures to prove that bytes match the declared MIME.
- Supported audio format labels include wav, mp3, m4a/mp4, ogg/opus, flac and webm in the relevant parser path.
- These limits mean the theoretical 16 × 8 MiB aggregate cannot fit in the 16 MiB encoded request cap. The effective request budget is lower and must be documented consistently.

A fresh implementation should enforce decoded-byte limits before allocating large buffers where possible, validate format/MIME policy explicitly, and test malformed base64, mismatched MIME, oversized encoded bodies and attachment-count boundaries.

## Per-Brain input declarations

The source's `advertised_input_modalities` currently returns:

| Brain ID | Declared input |
|---|---|
| `auto` | text, image, audio |
| `gemini` | text, image, audio |
| `chatgpt` | text, image |
| `claude` | text, image |
| `deepseek`, `kimi`, `mistral` | text, image |
| Other configured Brain IDs | text |

These are hard-coded values, not discovered from the UI at request time. The source comments and project docs are not entirely consistent about Claude audio: one catalog comment mentions image+audio, but the actual returned array for `claude` is `["text", "image"]`. Treat the function's actual return value as the API advertisement until the source and capability evidence are reconciled.

## Output modality paths

### Image generation

- `POST /v1/images/generations` uses a separate relay path, activates the image-generation mode in the browser UI, marks a baseline of existing generated image IDs, and looks for a new stable artifact. It does not treat any visible old image as a new successful result.
- The source supports one image per request (`n=1`) and returns base64 by default. A legacy `response_format=url` is represented as a local data URL, not a remotely hosted URL.
- `advertised_output_modalities` declares text+image for `chatgpt` and `auto`; other Brains are text-only.
- Auto-routing prefers ChatGPT then Gemini for image generation. The documentation notes that Gemini's UI had been seen to remain at “Creating your image” without an extractable new artifact in two recorded live runs. Thus `auto` output-image capability is conditional and can fail if its fallback is selected; it is not equivalent to a verified always-working image endpoint.

### Audio transcription and translation

- `POST /v1/audio/transcriptions` and `/v1/audio/translations` use multipart/form-data, parse the file and selected fields, then upload the audio through the chosen web Brain and return text.
- The source docs describe `json`, `text` and a conservative `verbose_json` response mode.
- This is a browser-assisted transcription/translation path, not OpenAI's native Whisper service. The returned text and timing/segment metadata cannot be assumed to match Whisper semantics.
- The route uses a custom multipart parser in the root bridge. It deserves dedicated malformed-boundary, binary-body, missing-field and size-limit tests; a mature multipart implementation would be preferable in a fresh HTTP layer.

### Audio speech generation

- `POST /v1/audio/speech` is routed, but the upstream docs explicitly say it fails closed with a provider error unless a Brain yields an extractable TTS audio artifact.
- A route existing in the server is not evidence that speech generation is supported. Do not advertise it as available until an artifact extraction path and a live end-to-end test pass.

## Capability evidence policy

The upstream catalog comments refer to live smoke markers such as `IMAGE_INPUT_OK` and `AUDIO_INPUT_OK`; the bridge advertises a modality only where its authors consider the relevant upload smoke verified. Those are historical claims in the source/docs and were not independently rerun in this audit.

For this project, use a per-Brain/per-operation evidence record with:
- exact Brain ID and web UI version/URL;
- test date and upstream commit;
- fixture file type, MIME, decoded size and prompt;
- whether the upload preview was visible;
- whether the model's answer demonstrably used the media (not merely acknowledged an attachment);
- latency/timeout and failure reason;
- whether the feature is stable enough to advertise.

Separate `supported`, `experimental`, `unverified` and `temporarily unavailable`. A rate limit/login failure should not be mislabeled as a permanent modality absence; an upload control that appears in the UI should not be mislabeled as a verified capability.

## Required tests

1. Base64 decode and MIME validation, including invalid padding, invalid alphabet, wrong prefix and empty bytes.
2. Exact boundary tests for 8 MiB per attachment, 16 attachments and 16 MiB encoded request size.
3. Reject remote URLs, file IDs, unknown blocks and unsupported media types.
4. Browser upload acceptance is verified by preview and by a response that demonstrably depends on the media.
5. Per-Brain capability gating rejects unsupported modalities before starting a browser turn.
6. Image generation must produce a new artifact ID/bytes, not an old image or UI icon.
7. Transcription/translation output formats are tested independently.
8. Speech route remains explicitly unsupported until real audio bytes are extracted and validated.
9. Auto-routing never selects a Brain that lacks the requested operation; fallback failures remain visible.
10. All evidence uses disposable profiles and redacted logs.

## Verification status

- **Source-verified:** accepted wire forms, request/attachment limits, MIME-prefix checks, per-Brain declarations, separate image/audio paths and current speech-route limitation.
- **Not verified:** current live media behavior, provider UI changes, successful uploads, generated artifacts or transcription accuracy.
