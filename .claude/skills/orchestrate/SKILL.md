---
name: orchestrate
description: >-
  Coordinate multiple implementation agents across GitHub issues. Use when asked
  to work several issues in parallel, drive an epic or wave of issues, or resume
  a multi-agent effort after an interruption or session limit.
---

# Orchestrating parallel implementation agents

You are the coordinator: you schedule issues, launch and resume agents, queue
PRs, and keep a running map of what is in flight. Agents implement; you never
implement in the orchestrator session.

## Concurrency budget

- **At most 2 implementation agents in flight at once.** A third buys almost no
  wall-clock time and burns the session limit, which kills every in-flight
  agent when it trips — the dominant failure mode of past orchestrations.
- **Model choice per issue:** well-specified leaf issues (spec is
  implementable cold, bounded file set) go to Sonnet agents; only
  cross-cutting or underspecified issues need the orchestrator's own model.
- Verification is the expensive part (`pre-push` ~4 min, web-smoke ~10 min per
  scenario, full baseline capture ~30 min — see `AGENTS.md`). Stagger agent
  launches so two agents are not both in a browser-matrix capture at the same
  time.

## Conflict-aware scheduling

Before launching anything, sketch each candidate issue's likely touch-set
(modules/files it will change).

- **Disjoint touch-sets → parallel is fine.**
- **Overlapping touch-sets (same subsystem: HUD, theme, palette, telemetry
  seams) → strictly sequential**, in dependency order. Two parallel PRs in the
  same subsystem force a rebase, and a rebase that changes rendered UI forces
  a full baseline re-capture — more expensive than just waiting.
- Tell every agent to rebase onto `origin/main` **before** its baseline
  capture so it captures exactly once.

## Launch contract

Every agent prompt must include:

1. The issue number and the statement that the issue body is the spec.
2. Its expected touch-set and which concurrent PRs (if any) it must not touch.
3. "Rebase before any baseline capture."
4. The delivery target: PR open with auto-merge queued (or explicitly not, if
   the orchestrator manages the queue), plus a **handoff report**: what
   changed, verification evidence, deviations/deferrals, and what the next
   issue needs to know.

## Resume protocol (session limits, interruptions)

Agents killed mid-flight leave valid work in their worktrees. On resume:

- **Never relaunch from scratch.** Use `SendMessage` to the existing agent (its
  context is intact) or, if it is gone, launch a new agent pointed at the
  existing worktree/branch with the last known state from its final report.
- First have the resumed agent check whether `main` moved while it was dead,
  and whether that invalidates work it already verified — cheapest check
  first (one checkpoint) before deciding on a full re-capture.
- Reconcile the board before launching anything new: which PRs merged, which
  worktrees are orphaned, which issues are still claimed.

## Queueing PRs

- Queue PRs with auto-merge as they land; when two queued PRs touch the same
  files, watch the second for a DIRTY state and trigger its rebase promptly.
- After each merge, re-check which blocked issues just became actionable and
  pull the next one — keep the pipeline full but never over budget.
