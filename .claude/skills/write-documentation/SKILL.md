---
name: write-documentation
description: >-
  Documentation writing for repo docs, READMEs, guides, runbooks, ADRs, API
  docs, and inline explanatory docs. Use when the user asks to write, update,
  edit, tighten, or review documentation.
---

# Write documentation in this repo

Write docs that help the reader act, then ship them through `tdd-ship` like
any other tracked change.

## Where docs live

- `README.md` — main index of the repository, detailing Bevy design, architecture, and setup.
- `docs/` — technical guides, assets documentation, and design specifications.

## Workflow

1. Identify the reader, task, and source of truth.
   Completion: you can name who reads the doc, what they need to do, and which
   files, code paths, specs, or user notes prove each claim.

2. Gather facts before drafting.
   Completion: every non-obvious claim is backed by current source material or
   marked as an assumption.

3. Draft around the task.
   Completion: the doc starts with the needed answer, then gives only the
   context, steps, decisions, or constraints the reader needs. Warnings and
   limits sit next to the step or fact they affect, not in a separate section.

4. Prune hard.
   Completion: each sentence changes what the reader knows or does; headings
   are not restated as prose.

5. Verify the result.
   Completion: commands, paths, option names, links, and examples are accurate;
   formatting and terminology match nearby docs.
