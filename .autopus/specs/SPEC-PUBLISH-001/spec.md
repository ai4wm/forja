# SPEC-PUBLISH-001: Prepare Forja for crates.io Publishing and Automated Releases

---
id: SPEC-PUBLISH-001
title: Prepare Forja for crates.io Publishing and Automated Releases
version: 0.1.0
status: completed
priority: HIGH
---

## Purpose

Make the Forja workspace publish-ready for crates.io and stabilize CI/CD around cross-platform builds, validation, and automated releases.

## Requirements

### Ubiquitous

The system shall complete package metadata required for publishing the `forja` CLI and its internal workspace crates.

The system shall provide CI workflows that validate the workspace on supported platforms without depending on undeclared local tools or models.

The system shall provide automated release workflows that build platform artifacts for Windows, macOS, and Linux and can publish crates in dependency order.

### Event-Driven

WHEN code is pushed or a pull request is opened THEN CI shall run deterministic workspace validation suitable for GitHub-hosted runners.

WHEN a release tag or explicit publish workflow is triggered THEN the system shall build release artifacts for supported targets and publish packages in dependency order with token-based authentication.

WHEN environment-dependent integration tests exist THEN CI shall clearly separate them from baseline publish-blocking validation.

### Unwanted

IF a workflow depends on local Ollama models or provider credentials THEN that dependency shall not block the baseline publish pipeline.

IF package metadata is incomplete THEN publishing shall fail in CI before the release step.

## Acceptance Criteria

- [ ] Root and internal crate manifests include publish-relevant metadata.
- [ ] CI validates the workspace across Windows, macOS, and Linux.
- [ ] Release automation builds cross-platform artifacts.
- [ ] crates.io publish automation exists or publish dry-run passes with a clear workflow.
- [ ] Environment-dependent tests are isolated from baseline CI.
