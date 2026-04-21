# Workspace Context

## Root

- Repository root: the current workspace root for the Forja Rust workspace
- Main build system: Cargo workspace with the root manifest at `Cargo.toml`

## Important Paths

- Source crates: `crates/forja-core/`, `crates/forja-llm/`, `crates/forja-memory/`, `crates/forja-tools/`, `crates/forja-channel/`
- Root runtime layer: `src/`
- Integration and phase tests: `tests/`
- Project and SPEC context: `.autopus/project/`, `.autopus/specs/`
- Architecture and status docs: `ARCHITECTURE.md`, `docs/`

## Working Conventions

- The repository may contain ignored Autopus artifacts under `.autopus/`, so sync steps may need explicit force-add behavior when those files must be versioned.
- Build artifacts are kept under `target/` or additional `target-*` directories.
- Verification in automation should run from the workspace root because several commands assume Cargo workspace-relative paths.

## Current Sync Notes

- `SPEC-DREAM-001` is the active synced workflow artifact for the dream-maintenance implementation.
- The workspace currently uses structured memory documents under the memory base directory, including `index.md`, `topics/`, `daily/`, `archive/`, `dreams/`, and `memory.db`.
