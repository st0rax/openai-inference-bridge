# Work contract

## Task ownership

1. Claim a task in `docs/TASKBOARD.json` before implementation: set `status=claimed`, `owner`, `branch`, and `claimed_at`.
2. Work on one task per branch.
3. Never start a task while any `depends_on` task is not `done`.
4. Keep diffs scoped; avoid opportunistic refactors.
5. Record verification commands and evidence in `proof_path`.
6. Mark `done` only after Definition of Done, tests and review are complete.

## Verification

For Rust changes, run the applicable subset of:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
git diff --check
```

Integration tasks must include reproducible client configuration, exact commands, redacted logs and observed results. Tests against mocks do not prove live web-chat compatibility.

## Evidence labels

Use `verified`, `partial`, `unverified`, or `blocked`. Never report a guessed or assumed behavior as tested. Do not claim full compatibility from a smoke test.

## Security and privacy

- Never commit API tokens, cookies, browser profiles, chat logs or credentials.
- Avoid logging prompt/response bodies by default.
- Redact tokens and session identifiers in evidence.
- Bind to loopback by default.
- Do not bypass a service's authentication or access controls.

## Completion checklist

- [ ] Claim fields and branch are present.
- [ ] Definition of Done is satisfied.
- [ ] Tests and formatting pass.
- [ ] Evidence is committed under `docs/proofs/`.
- [ ] No secrets or private session data are present.
- [ ] Taskboard and handover are updated.
