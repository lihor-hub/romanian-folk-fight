# Security Policy

Romanian Folk Fight is a hobby project: a browser-based, client-side arena
game with no server-side accounts, payments, or backend to speak of. That
keeps the attack surface small, but real issues are still worth reporting.

## Reporting a vulnerability

Preferred: use GitHub's private vulnerability reporting — go to the
[Security tab](https://github.com/lihor-hub/romanian-folk-fight/security) of
this repository and click **"Report a vulnerability"**. This opens a private
advisory only the maintainer can see.

If you'd rather not use GitHub, email ioachim.lihor@gmail.com with details
and, if possible, steps to reproduce.

Please don't open a public issue for a security report until it's been
triaged — that gives time to fix it before it's public.

## Scope

Roughly in order of relevance:

- The deployed web build at
  <https://lihor-hub.github.io/romanian-folk-fight/> (served via GitHub
  Pages) and anything in `deploy/` or `.github/workflows/deploy.yml` that
  produces it.
- Save-data handling — the game persists progress to `window.localStorage`
  in the browser and to a local file (via `dirs::data_dir()`) in native
  builds. There's no server-side storage and no accounts, so the main
  concerns here are things like save-data parsing causing a crash or a
  security-relevant panic on malformed/adversarial input, not data exposure.
- The CI/CD pipeline itself (GitHub Actions workflows under
  `.github/workflows/`) — e.g. workflow injection, secrets handling, or
  supply-chain concerns in how the site gets built and published.
- Dependency vulnerabilities in the Rust/Cargo dependency graph
  (`Cargo.lock`) that are actually reachable from this game's code.

Out of scope: this is a single-player game with no user accounts,
authentication, or personal data collection, so account-takeover, auth
bypass, and similar reports don't apply here. Denial-of-service against the
static GitHub Pages hosting is also out of scope — that's GitHub's
infrastructure, not this project's.

## What to expect

This is a one-person hobby project maintained in spare time — there's no SLA
and no dedicated security team. Reports are read and triaged on a best-effort
basis, typically within a few days. Fixes ship as soon as practical for
genuine issues; low-severity or purely theoretical reports may take longer or
be addressed as regular bug fixes instead of urgent patches.

Thanks for taking the time to report responsibly.
