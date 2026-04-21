# Forja Architecture

This document is a snapshot of the current implementation in this workspace.
It describes the code that exists today. It does not replace `docs/ARCHITECTURE.md`, which is a target-state design document for a later milestone.

## System Shape

Forja is a single-binary Rust workspace centered on the `forja` CLI in `src/main.rs`.
The root binary assembles five internal crates:

- `forja-core`: engine traits, runtime orchestration, prompt assembly, context, audit, budget, heartbeat, autonomy, dream maintenance orchestration, debate creation, and domain types
- `forja-llm`: multi-provider LLM client and provider presets
- `forja-memory`: markdown-backed structured memory storage and dream-maintenance persistence
- `forja-tools`: shell, file, web, search, browser, input, vision, and external CLI bridge tools
- `forja-channel`: CLI and Telegram channel adapters

The dominant architectural pattern is a single process with trait-based boundaries.
`forja-core` defines the contracts (`Channel`, `LlmProvider`, `MemoryStore`, `Tool`) and the root binary wires concrete adapters at startup.

## Runtime Assembly

The runtime startup path is now split across `src/runtime/startup.rs` and the `src/runtime/boot_*.rs` builder modules, with `src/main.rs` acting as the binary entrypoint and top-level command dispatcher.

1. Parse binary commands and flags.
   `forja login <provider>` dispatches to OAuth or token storage.
   `forja setup` runs the interactive provider setup wizard.
2. Load auth, config, and bootstrap profile data from `~/.forja`.
3. Build the combined system prompt from bootstrap files plus the first available project prompt file.
4. Select the active provider and model, then create the LLM client or mock provider.
5. Build runtime state for mode, memory, knowledge, audit, budget, heartbeat, dream maintenance, autonomy, and dashboard integration.
6. Register tools on the engine.
7. Attach the slash-command handler and start the streaming engine loop.

The resulting process is a CLI-first assistant that can optionally:

- run Telegram alongside the terminal
- open a local dashboard
- execute OS tools
- access browser automation and vision
- persist memory and audit state between runs

## Domain Boundaries

### 1. Entry and Composition

- `src/main.rs`: binary entrypoint, startup orchestration, tool registration, slash-command dispatch, and shutdown ordering
- `src/config.rs`: config schema, setup wizard, environment overrides, provider-to-config translation
- `src/provider_registry.rs`: model registry and `/model` resolution
- `src/bootstrap.rs`: identity and user bootstrap documents under `~/.forja`
- `src/oauth.rs`: OAuth login, token refresh, and auth persistence

This layer is composition-heavy and currently owns a large amount of runtime wiring.

### 2. Core Runtime

`forja-core` contains the main runtime logic and domain modules.

- `engine`: engine loop, tool recursion, streaming/non-streaming fallback, slash integration, memory loading, dream trigger handling, and shutdown hooks
- `prompt`: system prompt composition and role/think-mode prompt selection
- `mode`: execution mode, reasoning level, slash parsing, and image command detection
- `context`: token counting and context compression support
- `audit`: SQLite-backed audit logging
- `budget`: token budget tracking
- `heartbeat`: scheduled internal triggers
- `autonomy`: queued tasks, skill tracking, unresolved-task storage
- `engine/dream.rs`: idle/manual/shutdown dream trigger management and notification fan-out
- `creation`: debate engine and multi-agent synthesis flow
- `emotion`, `knowledge`, `serendipity`: user-context enrichment

This crate is the main domain boundary for all assistant behavior.

### 3. Adapter Crates

- `forja-llm`: provider-specific HTTP client logic and preset configuration
- `forja-memory`: structured markdown storage implementation for the `MemoryStore` trait, including `dreams/` logs and pending-journal recovery
- `forja-tools`: concrete tool adapters for shell, browser, input, vision, file, web, search, and external CLIs
- `forja-channel`: CLI and Telegram channel implementations

The root binary depends on these adapters and injects them into the engine at runtime.

### 4. Local Dashboard

- `src/dashboard/mod.rs`: local Axum server lifecycle and browser opening
- `src/dashboard/routes.rs`: read-only routes over `audit.db` plus task approval mutation
- `src/dashboard/static/index.html`: single-file dashboard UI

The dashboard is not a separate deployable service.
It is a local companion surface for the running CLI process.

## Data and Persistence

The runtime persists state under `~/.forja` and adjacent configured directories.

- `config.toml`: active provider/model, user settings, channel settings, dashboard port, and tool configuration
- `auth.json`: OAuth tokens
- `identity.md` and `user.md`: bootstrap identity and user profile
- `audit.db`: audit log, budgets, autonomy task queue, unresolved items, and learned skills
- `memory/index.md`, `memory/topics/`, `memory/daily/`, `memory/archive/`, `memory/dreams/`, and `memory/memory.db`: structured conversation memory, dream logs, and searchable summaries
- `knowledge/*.md`: knowledge context

Persistence is local-first.
There is no external database or service dependency required for the normal CLI flow.

## User-Facing Surfaces

### Binary Commands

- `forja`
- `forja setup`
- `forja login <provider>`
- `forja --provider <provider> --model <model>`

### Runtime Slash Commands

The currently implemented runtime command surface includes:

- `/mode`
- `/think`
- `/role`
- `/ss`
- `/image`
- `/debate`
- `/dashboard`
- `/skills`
- `/unresolved`
- `/task`
- `/dream`
- `/models`
- `/model`
- `/identity`

README coverage is incomplete relative to the implemented slash surface, so the code should be treated as the source of truth.

### Channels

- CLI is the primary interaction surface.
- Telegram can run in parallel through `MultiChannel` when configured.
- Discord is feature-gated in `forja-channel` but is not part of the active runtime described by the workspace today.

### Browser UI

The only browser-facing surface is the local dashboard.
It serves:

- `/`
- `/api/audit`
- `/api/debates`
- `/api/debate/:id`
- `/api/budget`
- `/api/skills`
- `/api/unresolved`
- `/api/tasks`
- `/api/approve/:id`

## Dependency Flow

The dependency direction is intentionally inward toward `forja-core` contracts.

- Root binary depends on all workspace crates.
- `forja-core` exposes the shared types and traits.
- `forja-llm`, `forja-memory`, `forja-tools`, and `forja-channel` implement concrete adapters.
- The dashboard depends on the same local SQLite data produced by the runtime.

At runtime the flow is:

`CLI/config/bootstrap -> provider/channel/tool construction -> Engine -> local persistence/audit/dashboard`

## Testing Layout

The repository uses a mix of:

- root integration and phase tests in `tests/`
- unit tests inside crates and modules
- provider-focused tests in `crates/forja-llm/tests/`
- dashboard route tests in `src/dashboard/tests.rs`

CI currently builds and tests primarily with `--no-default-features`, which means the default Telegram and vision paths are not fully covered by the baseline GitHub workflow.

## Observed Risks

- `src/main.rs`, `crates/forja-core/src/engine.rs`, and `src/dashboard/routes.rs` are large and exceed the project’s preferred file-size guidance.
- `README.md` and `docs/STATUS.md` are not fully aligned with the code that exists today.
- There is no dedicated `/health` or `/status` endpoint.
- There are no container or PaaS deployment manifests in the repository root.
- CI validates a reduced feature set with `--no-default-features`, so feature-enabled runtime paths need separate verification.
