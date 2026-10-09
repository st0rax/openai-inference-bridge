# Git and agent workflow

- `master` must remain green.
- Use one task per branch, named `<type>/<task-id>-<short-slug>`, e.g. `research/P-010-api-bridge-audit`.
- Claim a task before editing its files.
- Do not edit files outside the claimed task unless the task explicitly requires it.
- Prefer small commits with descriptive messages containing the task ID.
- Rebase/fast-forward merge where repository policy permits; do not force-push shared branches.
- After merge, update the task to `done`, attach evidence, and ensure the working tree is clean.
- Parallelize only tasks whose dependency sets do not overlap in a way that risks conflicting edits.
