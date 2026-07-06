---
name: report-issue
description: >-
  Create AFK-ready GitHub issues. Use when the user wants to report, file, log,
  or track a bug/feature for later instead of implementing it now. Do not use
  when the user wants the change shipped now; use tdd-ship.
---

# Report a problem → AFK-ready GitHub issue

Capture work as a **GitHub issue specified well enough that an agent can
implement it cold** from the issue text alone.

Use this when the user wants to *record* work, not do it now. If they want it
done immediately, use `tdd-ship` instead.

## AFK-ready contract

Before writing the issue, do the discovery the downstream agent would otherwise
redo, then bake the answers into the body.

A `ready-for-agent` issue MUST contain:

1. **Problem / goal** — what is wrong or wanted, and *why*. One or two sentences.
2. **Current state** — what exists today, with concrete `src/path/to/file.rs:line`
   references for every relevant call site or system. Name the functions, ECS systems, and modules involved.
3. **Scope** — a numbered list of the concrete changes to make. Be specific
   about which files/functions change and how. Call out anything explicitly
   *out* of scope or "leave as-is" so the agent doesn't over-build.
4. **Acceptance criteria** — a `- [ ]` checklist of verifiable outcomes. Each
   item should be objectively checkable (a test passes, a system behaves correctly), not a vibe.
5. **Test expectations** — what tests should back the change and roughly where
   they live.

If sections 2 or 3 are missing, investigate more or ask the question that
unblocks the spec. Do not file a vague issue.

## Workflow

### 1. Understand the report

Ground the report in the code. For a bug, identify where it goes wrong; for a
feature, identify where it slots in. Use file/search tools for every claim.

Only ask the user a question when the answer genuinely changes the spec and you
cannot derive it from the code. Otherwise pick the sensible default and note it in the
issue.

### 2. Choose labels

Always apply `ready-for-agent`. Add matching domain labels after checking
`gh label list`, e.g.:

- `bug` / `enhancement` — kind of work
- `ecs`, `gameplay`, `ui`, `assets`, `ci-cd` — area
- `documentation` — cross-cutting

If a needed label doesn't exist, create it. Never apply `ready-for-human` to an
AFK issue.

### 3. Create the issue

Write the body to a temp file (heredoc) to keep markdown intact, then:

```bash
gh issue create \
  --title "<type>: <concise imperative title>" \
  --body-file /tmp/issue.md \
  --label ready-for-agent \
  --label <domain-label>
```

Match the repo's commit voice in the title (`fix:` / `feat:` / `chore:`).

### 4. Report back

Give the user the issue URL, one-line summary, and labels. Make clear it is
queued for AFK pickup, not implemented now.

## Guardrails

- **Specify, don't implement.** This skill ends at a created issue. Touch no
  tracked source files. If implementation is wanted, hand off to `tdd-ship`.
- **One issue per logical change.** If the report bundles unrelated problems,
  file separate issues so each is independently shippable and reviewable.
- **Ground every reference.** Every `file:line` in the issue must be real and
  current — verify with the search tools before writing it.
- **No secrets in issue bodies.** Issues are world-readable on public repos.
