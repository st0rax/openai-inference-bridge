# Conversation state policy

Initial API requests are stateless at the bridge layer. Each request's complete normalized message history is composed into one prompt and submitted in a fresh browser chat. The bridge must not reuse a previous request's browser conversation as hidden context.

The browser profile may persist to retain login state. The bridge does not persist transcripts or responses. The web-chat provider may still store conversation history in the account; do not claim otherwise.

If a fresh, empty chat cannot be confirmed, fail or reset the backend before another request. Timeouts and disconnects may leave a partial provider-side conversation; never reuse that state.

No bridge-managed conversation IDs, conversation restore, cross-request memory, or Responses API persistence in the initial milestone.

Tests must cover distinct chats per request, complete ordered prompt history, stale draft detection, timeout/disconnect cleanup, login persistence without conversation reuse, and logs that omit full prompts/responses.