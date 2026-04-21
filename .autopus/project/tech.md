# Technology Stack

## Languages and Packaging

- Language: Rust
- Edition: 2024
- Workspace manager and build tool: Cargo
- Package layout: one root binary package plus internal workspace crates

## Runtime Libraries

- Async runtime: Tokio
- HTTP client: Reqwest
- Serialization: Serde and Serde JSON
- CLI prompts: Dialoguer
- Time and dates: Chrono
- Terminal progress/UI helpers: Indicatif
- Signal handling: Ctrl-C via `ctrlc`

## Local Web Stack

- HTTP server: Axum
- HTTP middleware: tower-http
- Dashboard UI: static HTML embedded with `include_str!`

## Storage

- Local structured storage: SQLite through `rusqlite` with bundled SQLite
- Memory store: structured markdown files plus `memory.db` and `dreams/` history managed by `forja-memory`
- Identity/config/auth storage: files under `~/.forja`

## Tooling Stack

- Shell execution: PowerShell or platform shell commands through the shell tool
- Browser automation: Chromium control through the browser tool
- Desktop input automation: input tool backends
- Vision and OCR: vision tool with optional screen capture backend
- Web and search access: web and search tools
- External bridge tools: Claude Code, Codex, and Gemini CLI wrappers when installed

## AI and Model Layer

- LLM abstraction lives in `forja-core` traits
- Concrete client lives in `forja-llm`
- Current provider set in the repository includes:
  - OpenAI
  - OpenAI OAuth
  - Anthropic
  - Gemini
  - Gemini OAuth
  - DeepSeek
  - GLM
  - Moonshot
  - xAI
  - Ollama

## Architectural Patterns

- Single-binary runtime composition
- Trait-based port and adapter boundaries
- Local-first persistence
- Feature-gated optional capabilities such as Telegram and vision
- Streaming-first assistant responses with fallback paths
- SQLite-backed observability for audit, budgets, and autonomy state
- Background dream maintenance using deterministic local rules over the structured memory layout
- Staged creation-engine execution with divergence, conflict, combination, mutation, convergence, and task synthesis over the existing budget and RALF policies

## Configuration Model

- Static config from `~/.forja/config.toml`
- OAuth token persistence in `~/.forja/auth.json`
- Environment variable overrides for provider, model, tool toggles, runtime behavior, and dream thresholds

## Build and Verification Tooling

- Local build: `cargo build --workspace`
- Local lint target: `cargo clippy --workspace`
- Required package tests called out by project rules:
  - `cargo test -p forja-llm`
  - `cargo test -p forja-llm -- --ignored`
- CI workflow: `.github/workflows/ci.yml`
- Release workflow: `.github/workflows/release.yml`

## Constraints Observed in Code

- CI primarily validates `--no-default-features`, so default-feature paths need separate verification.
- The runtime assumes a local filesystem and user home directory.
- There is no container, Kubernetes, or hosted deployment stack defined in the repository today.
