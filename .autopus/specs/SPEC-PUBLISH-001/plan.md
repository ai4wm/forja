# SPEC-PUBLISH-001 Plan

## File Impact

- `Cargo.toml`
- `crates/*/Cargo.toml`
- `README.md`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/workflows/publish.yml` or equivalent
- crate-level README files when needed for publish metadata

## Strategy

1. Normalize package metadata for root and internal crates.
2. Split baseline CI from credential/model-dependent validation.
3. Expand GitHub Actions to multi-OS validation and release artifact generation.
4. Add crates.io publish workflow with dependency-order publishing or explicit dry-run gate.

## Canary Context

- `cargo build --workspace` passed
- `cargo clippy --workspace -- -D warnings` passed
- `cargo test -p forja-llm` failed because local Ollama model `qwen3.5:9b` was unavailable
- `cargo test -p forja-llm -- --ignored` failed because one Gemini streaming test returned upstream 404

These failures should be treated as non-baseline integration coverage, not publish-blocking CI.
