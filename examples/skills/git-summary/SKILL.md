---
name: git-summary
description: Summarize the most recent git history
triggers:
  - git summary
  - summarize git
  - recent commits
scripts:
  - summary.sh
env:
tests:
  - name: basic output
    input: git summary
    expected_contains:
      - commit
  - name: no error
    input: git summary
    expected_not_contains:
      - error
      - fatal
---

# Git Summary

Steps:
1. Run the bundled summary script in the skill directory
2. Capture the recent commit list
3. Report the result clearly
