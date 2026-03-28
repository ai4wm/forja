# Forja Project Rules

## Work Principles
- Always inspect the current code structure before starting work.
- Analyze the impact scope before making changes.
- Modify only one file at a time.
- Submit a plan first and start coding only after approval.
- Ignore auto-approve. Only direct user approval is valid.

## Coding Rules
- Rust code comes first.
- Include tests with every change.
- Keep clippy warnings at zero.
- Do not maintain `MODEL_TABLE` and `models_for()` in parallel.
- Use inline variables in `format!` (`format!("{x}")` allowed, `format!("{}", x)` not allowed).
- Keep `match` exhaustive and minimize wildcard (`_`) arms.
- Do not create helper functions that are referenced only once.
- Keep modules under 500 lines when possible; split them if they exceed 800 lines.
- Prefer method references over closures when practical.
- Apply `collapsible_if` when `if` statements are nested.

## Test Rules (TDD)
- Write a failing test before modifying code.
- Pass all `cargo test -p forja-llm` tests before committing.
- Also run `cargo test -p forja-llm -- --ignored`.
- Always add a regression test for bug fixes.
- Do not commit test code under `crates/forja-llm/tests/`.

## Prohibited Actions
- Do not delete existing API keys from `config.toml`.
- Do not modify `C:\Users\homec\.forja\config.toml`.
- Do not modify `C:\Users\homec\.forja\auth.json`.
- Do not commit debug or log files such as `*.txt`, `*.log`, or `error.json`.
- Do not commit work logs such as `walkthrough.md`.

## Completion Criteria
- `cargo build --workspace` passes.
- `cargo clippy --workspace` reports zero warnings.
- `cargo test -p forja-llm` passes.
- Existing functionality still works.
- Check changed files with `git diff --name-only`.
- Confirm no unnecessary files such as `*.txt`, `*.log`, or `*.json` are included.

## Documentation Rules
- After every task, update `docs/STATUS.md` with:
  - Changed files
  - Feature status (`Done` / `Partial` / `Not started`)
  - Dependencies for the next task
- Keep all documentation in English only, except `docs/README.*.md` translation files.
- Do not write Korean or any non-English text in code or docs, except `docs/README.*.md` translation files.
