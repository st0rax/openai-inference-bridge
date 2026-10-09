# Start here

1. Read `GOALS.md`.
2. Read `docs/PLAN.md` and `docs/WORK_CONTRACT.md`.
3. Review `docs/UPSTREAM_REUSE.md` and `docs/UPSTREAM_COMPONENT_EVALUATION_POLICY.md` before implementation.
4. Open `docs/TASKBOARD.json`; select a task whose dependencies are all `done`.
5. Claim it with owner, branch, and timestamp before coding.
6. Keep changes limited to the task. Add tests and evidence.
7. Do not mark a task `done` until its definition of done and verification gates pass.

## First task

Start with `P-001` (repository skeleton) if this bootstrap has not yet been committed into the new repository. Otherwise start with `P-010` to audit the current bridge source.

## Ground rules

- Planning/bootstrap is not an implemented server.
- A protocol endpoint is not “fully compatible” merely because it accepts one example request.
- A brain capability is not available merely because the wire protocol has a field for it.
- Do not add `AgentController`, shell execution, autonomous planning, or `webagent/1` action handling to the inference path.
