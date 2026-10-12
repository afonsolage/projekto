---
name: review-pr
description: Watch a GitHub pull request's CI checks to completion, run the adversarial-review subagent when they pass, and post the findings as a GitHub review with inline comments on the changed lines. Use when asked to "watch PR #N", "check the PR checks and review it", "run the adversarial review on the PR", or "post a review/comment on the PR".
---

# Review a pull request

End-to-end workflow for `afonsolage/projekto`: gate on CI, run the adversarial review, then
publish the findings through GitHub's review feature. Follow `AGENTS.md` throughout, especially
its rules on CI monitoring and on posting comments/reviews exactly once.

## Inputs

- A pull request number or URL. If none is given, ask for one before doing anything else.
- Derive the repository once: `gh repo view --json nameWithOwner --jq .nameWithOwner`.

## Step 1 — Resolve the PR

```
gh pr view <n> --json number,title,state,isDraft,mergeable,headRefName,baseRefName,url,headRefOid,baseRefOid
```

Abort if the PR is closed or merged. If it is a draft, stop and confirm with the user unless they
already said to review it. Record `headRefOid`; the reviews API needs it as `commit_id`.

## Step 2 — Watch the checks

```
gh pr checks <n> --watch --interval 15
```

- `--watch` can exit on a transient GraphQL error. When that happens, do not restart blindly:
  poll `gh pr checks <n>` after a short `sleep` until every check reports `pass` (or `skipping`),
  or one reports `fail`/`cancelled`.
- This repo's checks are Format, Clippy, Test Suite and Unused Dependencies.
- If a check fails, stop. Investigate with `gh run view <run-id> --log-failed`, report the failure
  to the user, and skip the review. Say explicitly if the failure is pre-existing on `main`.

## Step 3 — Adversarial review

Only when every check passes. Spawn the `adversarial-review` subagent (Task tool,
`subagent_type: adversarial-review`). Give it:

- the PR number and URL, and the repo name;
- instructions to read `gh pr view <n>` and `gh pr diff <n>` and the full changed files;
- the project ground rules its agent definition already carries
  (`.opencode/agent/adversarial-review.md`);
- the required output: findings split into Blocking / Should-fix / Nit, each with `path:line`,
  plus an explicit statement when nothing is blocking and the residual risks.

The subagent is read-only. It must not edit files, commit, push, or post anything.

## Step 4 — Post a GitHub review

Post once. Never wrap `gh pr review`, `gh pr comment`, or the reviews API in a loop or an
automatic retry (AGENTS.md). Use the reviews API so findings attach to changed lines:

```
POST /repos/{owner}/{repo}/pulls/{n}/reviews
```

Payload (write it to a file and pass it with `gh api --method POST <path> --input <payload.json>`):

- `commit_id`: the PR's `headRefOid`.
- `event`: `"COMMENT"`. A self-review cannot be `APPROVE`.
- `body`: a short summary plus every finding that cannot be anchored inline.
- `comments`: `[{ "path", "line", "side": "RIGHT", "body" }]` — one entry per finding.

Anchoring rules:

- `line` is the new-file line number of an **added** line; `side` is `"RIGHT"`.
- A finding about a file that is not part of the diff cannot be an inline comment; put it in
  `body` instead, stating why it could not be anchored.
- Do not pad the review with comments that only confirm correct code.

Verify by reading back, not by re-running:

```
gh api /repos/{owner}/{repo}/pulls/{n}/reviews --jq '.[] | {id, state, commit_id}'
gh api /repos/{owner}/{repo}/pulls/{n}/comments --jq '.[] | {path, line, side, body}'
```

An empty stdout on the POST is a possible success, not a failure. Check the PR's reviews/comments
first and skip the call if the content is already there.

Include the disclosure AGENTS.md requires: state that the review is AI-assisted and that the
author owns it.

## Step 5 — Report

Return to the user:

- the PR URL;
- each check and its conclusion;
- the adversarial verdict (blocking vs non-blocking counts);
- what was published (review id and the inline comment locations);
- any follow-ups that were consciously deferred.

## Hard rules

- Do not post anything until CI is green, unless the user explicitly overrides.
- One review, each finding posted once; verify by reading back and never re-run a side-effecting
  command.
- Never commit, push, or edit files as part of this workflow.
- Cite `path:line` in every finding.
