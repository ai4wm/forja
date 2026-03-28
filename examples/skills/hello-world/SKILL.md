---
name: hello-world
description: Print a simple greeting message
triggers:
  - hello
  - greet
  - greeting
scripts:
env:
tests:
  - name: no script fallback
    input: hello
    expected_contains:
      - Skill has no scripts to execute.
---

# Hello World

This sample skill is intentionally minimal.

Steps:
1. Confirm that the skill loader can discover this skill
2. Return a short greeting to verify the skill pipeline
