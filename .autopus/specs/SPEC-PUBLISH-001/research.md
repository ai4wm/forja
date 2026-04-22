# SPEC-PUBLISH-001 Research

## Findings

- The workspace already has `ci.yml` and `release.yml`, but CI is Ubuntu-only and release automation does not cover crates.io publishing.
- Existing `forja-llm` tests depend on external models/credentials and currently fail in the canary baseline.
- Package metadata is minimal across the workspace and needs publish-oriented completion.

## Recommended Shape

- Keep baseline CI deterministic and cross-platform.
- Move external-provider streaming checks into optional/manual jobs.
- Add a dedicated publish workflow rather than overloading the artifact release workflow.
