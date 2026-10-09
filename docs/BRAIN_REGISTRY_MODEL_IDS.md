# Brain registry and model IDs

Status: P-021 design proposal, 2026-10-09. No registry implementation is claimed.

## Decisions

- A **Brain** is a configured browser-chat backend (for example, a ChatGPT web session), not a provider API model name.
- The registry is project-owned configuration. It does not use WebAgent's Brain registry or types.
- Each enabled Brain exposes one canonical OpenAI-compatible model ID: `oib/<brain-id>`.
- `<brain-id>` is a stable lowercase ASCII slug matching `[a-z0-9][a-z0-9-]{0,62}`. IDs are unique after lowercase normalization. Reject invalid or duplicate IDs at startup.
- The canonical ID is derived from the configured Brain ID, not its display name, URL, account email, selected provider model, or profile path.
- No `auto`, implicit default, provider-native aliases or fuzzy matching in the initial release. A request must select an exact registered model ID.
- `/v1/models` lists configured and enabled Brains without launching browsers or probing login state. Runtime readiness is a separate per-Brain status.
- Unknown IDs fail as model-not-found; never silently route the request to another Brain.

## Example

A configuration entry with Brain ID `chatgpt` produces model ID `oib/chatgpt`. The OpenAI API adapter maps this to the internal Brain ID `chatgpt`; it must not tell clients that the browser session is a particular underlying model unless that capability/model selection has been independently verified.

## Registry record

The initial internal record should contain only the minimum needed to resolve a request:

- stable `BrainId`;
- enabled/disabled state;
- human-readable display label (not used as an identifier);
- configured web-chat start URL;
- provider-adapter kind or selector-set reference;
- app-owned profile identity/path resolved by configuration code, not accepted from an API request.

Capability declarations are specified in P-022. Secret/config parsing and path rules are specified in P-025. Keep provider selectors out of model IDs. Do not allow an API caller to supply a URL, filesystem path or selector override.

## Startup validation

Reject startup on:
- malformed Brain IDs or non-canonical casing;
- duplicate IDs or canonical model IDs;
- empty or malformed start URLs;
- unknown adapter/selector-set identifiers;
- enabled entries without required configuration.

Disabled entries are omitted from `/v1/models`. Do not silently repair invalid configuration into a different ID.

## Model-list semantics

The API layer projects each enabled registry entry into the OpenAI model-list wire shape. That projection is not part of the registry contract. The registry does not perform HTTP serialization and does not store API response JSON.

Listing a model means only that it is configured and enabled. It does not promise that its browser is running, logged in, unblocked, or currently available. Those are runtime readiness states.

## Tests required

1. Stable canonical ID generation and round-trip model-to-Brain resolution.
2. Invalid, uppercase, empty, too-long and path-like IDs rejected.
3. Duplicate IDs rejected after normalization.
4. Display-label or URL changes do not change the canonical ID.
5. Unknown IDs are not routed to a default Brain.
6. Disabled Brains are omitted from model listing.
7. Listing models does not start a browser or inspect a profile.
8. Registry records and serialized errors never expose profile paths or secrets.

## Deferred

Aliases, dynamic discovery, auto-routing, per-provider model switching, live readiness in model listing, and registry hot reload are out of scope until separate requirements and tests exist.
