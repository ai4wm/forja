# SPEC-PUBLISH-001 Acceptance

## Scenarios

### Scenario 1: Multi-platform CI

Given a pull request  
When GitHub Actions runs  
Then the workspace shall validate on Windows, macOS, and Linux with a deterministic baseline job set

### Scenario 2: Release artifacts

Given a release tag  
When the release workflow runs  
Then Windows, macOS, and Linux artifacts shall be built and attached to the release

### Scenario 3: Publish workflow

Given a manual or tagged publish trigger  
When the publish workflow runs  
Then crates shall publish in dependency order or fail early in dry-run validation

### Scenario 4: Environment-dependent tests isolation

Given provider or model-dependent tests  
When baseline CI runs  
Then those tests shall not fail the default publish gate
