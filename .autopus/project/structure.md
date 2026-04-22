# Repository Structure

## Workspace Layout

The repository is a Rust workspace with one root binary package and five internal crates.

## Top-Level Directories

- `src/`: root binary entrypoint and local dashboard code
- `crates/forja-core/`: core engine, domain logic, traits, and shared runtime types
- `crates/forja-llm/`: LLM client and model/provider preset logic
- `crates/forja-memory/`: markdown memory store
- `crates/forja-tools/`: shell, browser, input, vision, search, file, web, and CLI bridge tools
- `crates/forja-channel/`: CLI, Telegram, and Discord channel implementations
- `tests/`: phase and integration-style runtime tests
- `docs/`: roadmap and architecture/status documentation
- `.github/workflows/`: CI and release automation
- `.autopus/`: harness context and generated workflow data
- `.agents/`: shared skills and plugin metadata for agent tooling
- `examples/skills/`: example skill assets

## Root Binary Files

- `src/main.rs`: binary entrypoint, runtime composition, slash-command handling, tool registration, startup, and shutdown
- `src/config.rs`: config schema, setup wizard, environment overrides, and provider resolution
- `src/provider_registry.rs`: model table and `/model` lookup logic
- `src/oauth.rs`: OAuth login and token refresh
- `src/bootstrap.rs`: bootstrap identity and user files

## Dashboard Files

- `src/dashboard/mod.rs`: local Axum server lifecycle
- `src/dashboard/routes/`: modular dashboard API routes and asset handlers
- `src/dashboard/static/`: dashboard HTML, CSS, and JavaScript assets
- `src/dashboard/tests.rs`: dashboard route tests

## Crate Responsibilities

### `forja-core`

Key module groups:

- `engine/`
- `prompt/`
- `context/`
- `audit/`
- `budget/`
- `heartbeat/`
- `autonomy/`
- `creation/`
- `gateway/`
- `ralf/`
- `emotion.rs`
- `knowledge.rs`
- `mode.rs`
- `serendipity.rs`
- `traits.rs`
- `types.rs`

This crate is the center of the runtime model.
Its runtime decomposition now includes `engine/dream.rs` for idle/manual/shutdown dream orchestration.
The creation subsystem is now split across `creation/debate.rs`, `creation/combination.rs`, `creation/mutation.rs`, `creation/execution.rs`, and dedicated creation test modules.

### `forja-llm`

Key files:

- `src/client.rs`
- `src/config.rs`
- `src/models.rs`
- `src/presets.rs`
- `tests/`

This crate isolates provider-specific LLM behavior and test coverage.

### `forja-memory`

Key files:

- `src/lib.rs`
- `src/storage/mod.rs`
- `src/storage/dream.rs`
- `src/storage/journal.rs`
- `src/sqlite.rs`
- `src/session.rs`

This crate provides the concrete structured markdown-backed memory implementation, including `dreams/` logging and pending-journal recovery.

### `forja-tools`

Key files:

- `src/shell.rs`
- `src/browser.rs`
- `src/input.rs`
- `src/vision.rs`
- `src/search.rs`
- `src/file.rs`
- `src/web.rs`
- `src/mcp/`
- `src/bin/forja-mcp.rs`
- `src/claude_code.rs`
- `src/codex.rs`
- `src/gemini_cli.rs`

This crate contains the execution surface used by the agent runtime and an MCP server surface for external agents.

### `forja-channel`

Key files:

- `src/cli.rs`
- `src/discord.rs`
- `src/multi.rs`
- `src/telegram.rs`

This crate owns channel input/output and multi-channel coordination.

## Test Distribution

- `tests/phase*.rs`: end-to-end or subsystem-focused runtime behavior tests
- `crates/forja-llm/tests/`: provider and streaming tests
- in-module `#[cfg(test)]`: unit tests for focused components

## Entry Points

- Binary entrypoint: `src/main.rs`
- Dashboard server entrypoint: `src/dashboard/mod.rs`
- Root workspace manifest: `Cargo.toml`
- CI entrypoints: `.github/workflows/ci.yml` and `.github/workflows/release.yml`

## Notes on Current Shape

- The active codebase is CLI-first.
- The dashboard is embedded in the root binary instead of being split into a separate service.
- Some large files still concentrate too much responsibility, especially `src/main.rs` and `crates/forja-core/src/engine.rs`.
