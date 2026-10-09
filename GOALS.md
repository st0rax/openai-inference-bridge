# Goals and non-goals

## Goal

Build a standalone application that exposes authenticated browser-chat interfaces as selectable inference “models” through an OpenAI-compatible API. The web-chat interface is the brain runtime; the API makes that brain consumable by external clients and harnesses.

## Core concepts

- **Brain:** a particular web-chat interface/session backend, such as a configured ChatGPT or Claude browser session.
- **Model ID:** the stable public identifier selected by an API client; it maps to a Brain.
- **Inference API:** the HTTP boundary consumed by clients.
- **Brain adapter:** the internal interface between the API service and a specific browser-chat implementation.
- **Harness/client:** an external application such as Pi, OpenCode, OpenClaw or ZeroClaw that sends ordinary inference requests.

## Goals

1. Keep the API independent of any one client/harness.
2. Reuse selected browser-session and inference code from `webagent-rs` after a file-level audit.
3. Separate API DTOs, normalized internal types, model catalog, runtime adapter, transport, and security boundary.
4. Support OpenAI-style model discovery, chat completions, streaming, and correct errors as a tested baseline.
5. Maintain per-Brain capability declarations and honest compatibility matrices.
6. Provide a small web UI to inspect configured Brains and test chats; it is an operations/test console, not a replacement harness.
7. Make build, tests, and compatibility evidence reproducible.

## Non-goals

- Reimplementing Pi, OpenCode, OpenClaw, ZeroClaw, or another harness.
- Adding autonomous agent loops, planning, shell execution, file editing, or the `webagent/1` action protocol to the inference path.
- Copying the entire `webagent-rs` repository or its benchmark/research/workers subsystems.
- Claiming full OpenAI compatibility before endpoint-by-endpoint testing.
- Claiming that every Brain supports images, audio, tools, JSON mode, or cancellation without runtime evidence.
- Circumventing authentication, access controls, or technical restrictions of a web-chat service.
