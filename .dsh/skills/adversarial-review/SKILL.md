---
name: adversarial-review
description: Run Projekto's adversarial review on a change or PR before asking for human review. Use when AGENTS.md requires the adversarial review, when asked to "run the adversarial review", or when the review-pr skill reaches its review step.
---

# Adversarial review (dsh adapter)

The reviewer's instructions live in `.opencode/agent/adversarial-review.md`, shared with opencode.
That file is the source of truth; do not copy or paraphrase it here.

1. Read `.opencode/agent/adversarial-review.md` and drop its YAML frontmatter.
2. Start one delegated agent with the `subagent` tool. Its task is the body of that file,
   followed by the scope to review: the base and head refs (or PR number), the changed files, and
   what the change is meant to do.
3. Tell it explicitly that it is read-only: no file edits, commits, pushes or GitHub posts. It may
   run read-only commands, `cargo check -p <crate>` and `cargo test -p <crate>`, following the
   build and disk rules in `AGENTS.md`.
4. When it finishes, relay its Blocking, Should-fix and Nit findings with `path:line` references.
   Do not fix anything unless asked.

If the `subagent` tool is not available in this session, carry out the review directly with the
same instructions and the same read-only rule.
