---
name: tdd-ship
description: >-
  Default delivery workflow for tracked repo changes. Use when the user asks to
  fix, add, implement, update, refactor, rename, bump, tweak, or otherwise end
  in a file change. Do not use for read-only questions or investigation.
---

# TDD → Issue → PR → CI → Merge

This is how tracked changes ship in this repo: issue -> branch -> red/green
TDD -> review -> PR -> CI -> auto-merge.

## Establish the delivery contract

Before editing, derive a compact working contract from the request, referenced
issue/PR, repository instructions, and existing code:

- **Objective** — the observable result the user wants.
- **Scope** — allowed files, systems, and logical changes; name exclusions.
- **Authority** — investigation only, local edit, PR delivery, or merge.
- **Invariants** — behavior, data, formatting, and unrelated work to preserve.
- **Acceptance** — evidence that distinguishes done from partially done.
- **Verification** — focused tests, repository gates, and manual/visual checks.
- **Terminal state** — the point where work may stop.

State material assumptions in the first progress update. Do not turn this into a
ceremonial questionnaire or wait for approval when the issue, PR, code, or repo
instructions answer the question. Ask only when a missing choice would
materially change the result or require authority outside the request.

Terminal-state examples:

| Request | Terminal state |
| --- | --- |
| “Investigate/explain” | Evidence-backed report; no edits |
| “Fix locally; no PR” | Verified working-tree change |
| “PR this” | PR open with requested checks reported |
| “CI pass” | Required checks green; failures fixed or exact blocker reported |
| “Merge” | Merge queue landed; `mergedAt` confirmed |

This repo's full tracked-change pipeline defaults to confirmed merge on green
CI when the user does not name a narrower terminal state. “PR this” narrows the
terminal state to an open PR; “CI pass” narrows it to green required checks;
“merge” requires confirmed merge. Phrases that describe intermediate mechanics,
such as “rebase and push back,” do not narrow the default unless the user says
to stop there or forbids later steps. Never silently stop before the applicable
terminal state.

## When this applies

Apply the full pipeline when the request modifies a tracked file: code, config,
docs, tests, or deploy files.

Skip it for pure questions, explanations, research, or read-only investigation
("how does X work?", "where is Y defined?", "explain this") — answer those
directly. If a question turns into "…so fix it," the pipeline kicks in at that
point.

If the user explicitly asks to edit locally without a PR, honor that scope but
keep the branch, tests, and reporting discipline.

## The pipeline

Run these in order.

### 0. Protect local work and synchronize

Before creating an issue or changing repository state, inspect `git status` and
run `git fetch origin`. Existing changes belong to the user. Keep them out of
the task's commits and diffs. Work around them in the current worktree when
safe; otherwise use a clean worktree or stop and report the exact overlap.
Never stash, discard, rewrite, or include unrelated changes merely to obtain a
clean tree.

Rebase the applicable clean current branch, existing PR branch, or detached
worktree state onto `origin/main` before starting delivery work. Rebase again in
Step 5 when the base advances before push. Do not create an issue, branch, edit,
or commit from a stale base.

### 1. Open or reuse a GitHub issue

Capture the intent before writing code.

If the user references an existing issue, use it as the source of truth. If the
work is an existing PR repair, use the PR and its linked issue; do not create a
duplicate tracking issue merely to satisfy this step. Create a new issue only
when the change lacks an adequate tracker.

```bash
gh issue create --title "<concise imperative title>" \
  --body "<what & why; acceptance criteria as a checklist>" \
  --label "ready-for-agent"
```

Keep the title in the repo's voice (`fix:`/`feat:` style matches commits).
Always apply `ready-for-agent`; the issue body needs context, file references,
scope, acceptance criteria, and test expectations.

### 2. Branch off an up-to-date `main`

Never commit to `main` directly, and never start from a stale base. Sync with
`origin/main` first, then branch:

```bash
git fetch origin
git switch main && git pull --ff-only
git switch -c <type>/<short-slug>-<issue#>   # e.g. feat/player-movement-123
```

If you're already on a feature branch for this work (e.g. a worktree where you
    can't `git switch main`), stay on it — but rebase it onto the freshly fetched
base before you write anything: `git fetch origin && git rebase origin/main`.
Starting from an up-to-date `main` keeps the diff clean and avoids merge
conflicts when you push.

### 3. TDD — write the failing test first

- **Red**: add/modify a test that encodes the new behavior. Run it; confirm it
  fails because the behavior is missing (not because of a typo or import error).
- **Green**: write the minimum implementation to make it pass.
- **Refactor**: clean up code and test while keeping the suite green.

Match the repo's test conventions and run the gates locally before pushing —
defer to the `rust-dev` skill for exact commands and patterns:

- During implementation, run focused Rust tests.
- For UI changes, run exactly one most-relevant browser scenario.

In a fresh worktree, run `scripts/bootstrap-worktree.sh` first to verify path and setup.

Push only after the relevant tests and type/lint gates pass locally.

Before every push, run `cargo xtask pre-push`. Run additional browser scenarios
only when directly affected. Do not run `cargo xtask web-smoke --all` locally
unless shared theme/layout infrastructure changed, visual baselines across
multiple screens changed, or the user explicitly requests the full browser
matrix. CI owns the exhaustive browser suite.

### 4. Review and verify the diff

CI only re-runs the tests you just wrote, so the diff needs one independent
check before it heads for auto-merge:

- Run the `review` skill on the working diff when available and fix confirmed
  findings before pushing. If it is unavailable, perform one focused diff review
  against the delivery contract and repository standards.
- For user-facing changes (UI, game mechanics), verify the change working in the running app (e.g. build and run locally).

Completion: every confirmed finding is fixed or explicitly dismissed with a
stated reason. Don't chase speculative findings, and don't loop — one review
pass, one fix pass, done.

### Configuration and generated-text preservation

For YAML, TOML, JSON-with-comments, deployment manifests, documentation indexes,
and similar structured text, treat unrelated representation as an invariant
unless normalization is explicitly requested.

- Prefer the narrowest edit that changes only allowed fields or paths.
- Preserve comments, blank lines, ordering, quoting, anchors, block scalars,
  whitespace, and unrelated content.
- Do not deserialize and rewrite a whole document when that creates unrelated
  churn, even if the result is semantically equivalent.
- Add a fixture or diff regression test for repeatable transformations. Assert
  both the intended semantic change and preservation of unrelated text.
- Inspect the final diff; validity checks alone do not prove preservation.

If removing child fields may empty a parent mapping, define the expected parent
behavior in the delivery contract and test it explicitly.

### 5. Rebase on `origin/main`, open or update the PR, and queue when applicable

Rebase immediately before pushing, re-run gates if the rebase changes the base,
push, then open a new PR or update the existing PR. Queue auto-merge only when
the delivery contract's terminal state is merge:

```bash
git fetch origin && git rebase origin/main
git push -u origin HEAD
gh pr create --fill --base main \
  --body "Closes #<issue#>\n\n<summary of the change and the test that backs it>"
# Run only when merge is the terminal state:
gh pr merge --squash --auto
```

After rebasing a published PR branch, update it with
`git push --force-with-lease`; never use plain `--force`. A normal fast-forward
push remains preferred when rewriting remote history is unnecessary.

The `Closes #<issue#>` line is required. End the PR body with the standard trailer:
`🤖 Generated with Antigravity`.

When merge is the terminal state, enable auto-merge (`--auto`). Branch
protection uses a merge queue; do not pass `--delete-branch` with queued merges.
For a PR-only or CI-only terminal state, leave auto-merge disabled unless the
user explicitly asks to queue it.

Do not bypass the queue with `gh pr merge --admin` or a direct push to `main`.

### 6. Watch CI and the merge queue as required by the contract

CI (`.github/workflows/ci.yml`) runs on every PR to `main`. Watch it until the
applicable terminal state is reached:

```bash
gh pr checks --watch
```

If a check fails, read the logs (`gh run view <run-id> --log-failed`), fix the
cause on the branch, push, and let CI re-run.

Report meaningful state changes: CI started, a check failed, all checks passed,
the PR entered the queue, the queue failed, or the merge landed. Do not narrate
routine commands individually or post a message for every poll. If active agent
instructions require an update while external state is unchanged, send one
compact heartbeat at the longest interval those instructions allow.

### 7. Confirm the merge completed when merge is the terminal state

Once CI is green, confirm auto-merge landed (`gh pr view <pr#> --json
state,mergedAt`), confirm the issue closed, delete the remote branch if GitHub
did not, and report the PR/issue numbers.

## Reporting back

At the applicable terminal state, summarize with links to the issue and PR.
Report whether checks are pending, green, or failed; when merge applies, also
confirm the merge time and branch cleanup. If anything blocked the pipeline
(flaky CI, a check that needs secrets, a merge conflict), surface it instead of
silently stopping.

Every final report must make the terminal state auditable: identify what
changed, list verification evidence, link the issue/PR when applicable, and
state the exact remote status. When merge applies, include the confirmed merge
time; when blocked, include the exact blocker. Do not claim completion from
local checks when the requested terminal state depends on remote CI or merge
state.

## Guardrails

- **Authorization to merge is built in when merge is the applicable terminal
  state** — the user has opted into auto-merge on green CI for this repo, so you
  don't need to re-ask. A PR-only or CI-only request is not merge authorization.
  Never force-merge past a failing or pending required check.
- **One issue/PR per logical change.** If a request bundles unrelated changes,
  split them so each PR stays reviewable.
- **If the user explicitly says "just edit, don't open a PR"** or is clearly
  working in a throwaway/experimental context, honor that and skip the pipeline
  for that request.

## Red flags

- Starting edits without knowing where the task is allowed to stop.
- Asking the user for facts already available in the issue, PR, repo, or code.
- Treating valid parsed output as proof that a configuration rewrite is safe.
- Including unrelated dirty-worktree changes to save time.
- Reporting “done” while requested CI or merge state is still pending.
- Repeating unchanged CI status on every poll.

Any red flag means pause, re-establish the delivery contract, and correct the
workflow before proceeding.
