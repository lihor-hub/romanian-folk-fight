# Save-Reload CDP Death Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `save-reload` browser smoke tolerate one proven Chrome/CDP process death while preserving immediate failures for product assertions and retaining actionable Chrome crash/exit evidence.

**Architecture:** Keep retry policy local to `save_reload.rs`: execute the complete journey at most twice, and only classify the exact `headless_chrome` closed-connection signature as retryable. Extend `browser.rs` launch configuration so Chrome writes its debug log and crash dumps beside the attempt artifacts, and snapshot the Chrome PID/process state before a failed attempt drops the browser handle.

**Tech Stack:** Rust 2024, `headless_chrome` 1.0.22, Chrome DevTools Protocol, cargo-xtask.

## Global Constraints

- Touch only the save-reload browser harness/CDP lifecycle, Chrome diagnostics/artifacts, and tests.
- Do not touch theme, UI layout, shop rows, character rendering, or unrelated browser scenarios.
- Retry one complete save-reload scenario attempt only for the exact known CDP closed-connection signature.
- Never retry assertions, malformed save data, console/page errors, timeouts, or near-match transport text.
- Preserve the first attempt at `save-reload/journey`; retain a retry, when needed, under `save-reload/journey-retry`.
- Run `cargo xtask web-smoke --scenario save-reload` and `cargo xtask pre-push` before push.

---

### Task 1: Pin the retry classification and budget

**Files:**
- Modify: `xtask/src/web_smoke/save_reload.rs`
- Test: `xtask/src/web_smoke/save_reload.rs`

**Interfaces:**
- Consumes: scenario attempt closures returning `Result<T, String>`.
- Produces: `is_cdp_connection_death(&str) -> bool` and `run_with_cdp_retry(F) -> Result<T, String>`.

- [x] **Step 1: Write failing classifier tests**

Add tests that accept `waiting for an animation frame failed: Unable to make method calls because underlying connection is closed`, reject ordinary assertion text, and reject generic/near-match “connection closed” text.

- [x] **Step 2: Write failing retry-budget tests**

Use a closure with an attempt counter to prove: one CDP death gets one retry; two CDP deaths stop after two total attempts; an assertion stops after one attempt; an assertion after a CDP retry stops without a third attempt.

- [x] **Step 3: Verify red**

Run: `cargo test -p xtask web_smoke::save_reload::tests -- --nocapture`

Expected: FAIL because the classifier and retry wrapper do not exist.

- [x] **Step 4: Implement the minimum pure retry policy**

Match only the exact `headless_chrome` phrase `Unable to make method calls because underlying connection is closed`; set the retry budget to one; return every non-matching error immediately.

- [x] **Step 5: Verify green**

Run: `cargo test -p xtask web_smoke::save_reload::tests -- --nocapture`

Expected: all save-reload policy tests pass.

### Task 2: Retain Chrome crash and process evidence

**Files:**
- Modify: `xtask/src/web_smoke/browser.rs`
- Test: `xtask/src/web_smoke/browser.rs`

**Interfaces:**
- Consumes: the existing attempt-local `chrome-profile` path.
- Produces: `chrome_debug.log`, `chrome-crashes/`, and `chrome-process.log` in the attempt directory.

- [x] **Step 1: Write failing diagnostic-routing tests**

Assert that the launch diagnostic paths live beside `chrome-profile`, the Chrome environment routes `CHROME_LOG_FILE` and `BREAKPAD_DUMP_LOCATION` to those paths, and the crate default `--disable-breakpad` argument is ignored.

- [x] **Step 2: Verify red**

Run: `cargo test -p xtask web_smoke::browser::tests -- --nocapture`

Expected: FAIL because launch diagnostics are not configured.

- [x] **Step 3: Implement launch diagnostics**

Create the dump directory, enable Chrome file logging, set the two diagnostic environment variables, and exclude `--disable-breakpad` from `headless_chrome` defaults without changing rendering flags.

- [x] **Step 4: Add process-state capture**

Expose a `Checkpoint` diagnostic writer that records the Chrome PID, `/proc/<pid>/status` and `/proc/<pid>/stat` when available, plus a portable `ps` snapshot before the browser handle is dropped.

- [x] **Step 5: Verify green**

Run: `cargo test -p xtask web_smoke::browser::tests -- --nocapture`

Expected: browser unit tests pass.

### Task 3: Wrap only save-reload in the retry policy

**Files:**
- Modify: `xtask/src/web_smoke/save_reload.rs`

**Interfaces:**
- Consumes: `run_with_cdp_retry`, `Checkpoint::write_process_diagnostics`.
- Produces: first attempt in `journey`, optional second attempt in `journey-retry`.

- [x] **Step 1: Extract one complete journey attempt**

Keep all current navigation, save, reload, and assertion code intact inside one attempt function. On any attempt error, write process evidence before `Checkpoint` drops.

- [x] **Step 2: Add the scenario-level wrapper**

Run the first attempt in `journey`; only an exact CDP death starts `journey-retry` with a fresh profile/browser. Print the retry reason and both artifact locations.

- [x] **Step 3: Run focused tests**

Run: `cargo test -p xtask web_smoke::save_reload::tests web_smoke::browser::tests -- --nocapture`

Expected: all focused tests pass.

- [x] **Step 4: Run the real browser journey**

Run: `cargo xtask web-smoke --scenario save-reload`

Expected: PASS; `journey/chrome_debug.log` exists, and the full reload resumes at Shop with wallet, ladder, and purchase intact.

### Task 4: Review and deliver

**Files:**
- Create: `.superpowers/sdd/issue-312-report.md`
- Modify if findings require: `xtask/src/web_smoke/browser.rs`, `xtask/src/web_smoke/save_reload.rs`

**Interfaces:**
- Consumes: issue #312, the branch diff, and fresh verification output.
- Produces: an auditable report and a queued PR closing #312.

- [ ] **Step 1: Run independent standards and spec reviews**

Review `git diff origin/main...HEAD` against `AGENTS.md`, issue #312, and the exclusive touch-set. Fix confirmed findings once.

- [ ] **Step 2: Write the required report**

Record the three-attempt evidence, root cause, classifier/retry semantics, diagnostic artifacts, tests/browser result, review findings, commits, PR state, deviations, and #330 rerun instructions.

- [ ] **Step 3: Rebase and run the full gate**

Run: `git fetch origin && git rebase origin/main`

Then run: `cargo xtask pre-push`

Expected: exit 0 with fmt, clippy, tests, and build matrix green.

- [ ] **Step 4: Commit, push, and queue**

Push `codex/fix-save-reload-cdp-312`, create a PR whose body contains `Closes #312` and ends with `🤖 Generated with Antigravity`, then run `gh pr merge --squash --auto`.

- [ ] **Step 5: Report the unblock**

State whether #330 can be rebased/rerun after #312 lands without cherry-picking and do not modify #330.
