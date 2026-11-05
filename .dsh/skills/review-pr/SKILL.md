---
name: review-pr
description: Watch a GitHub pull request's CI checks to completion, run the adversarial review when they pass, and post the findings as a GitHub review with inline comments on the changed lines. Use when asked to "watch PR #N", "check the PR checks and review it", "run the adversarial review on the PR", or "post a review/comment on the PR".
---

# Review a pull request (dsh adapter)

Follow `.opencode/skills/review-pr/SKILL.md` exactly; it is the source of truth, shared with
opencode. One substitution applies in dsh:

- Where it says to spawn the `adversarial-review` subagent with the Task tool, load and follow the
  `adversarial-review` skill instead. It starts the same reviewer through dsh's `subagent` tool.

Everything else (CI gating, posting the review exactly once, verifying it with
`gh pr view <number> --json reviews`) is unchanged.
