# CLAUDE.md

This repository keeps a **single source of truth** for agent and contributor
guidance: [AGENTS.md](AGENTS.md).

See AGENTS.md for the project scope, code style, build/deploy process, the
ship-change workflow, and how skills/hooks are wired for both Claude Code and
Codex. This file is intentionally a redirect only — do not duplicate AGENTS.md
content here. A pre-commit check (`.agents/hooks/check-agent-sync.mjs`) enforces
that this file keeps pointing at AGENTS.md.

## Keeping this document current

Keep this file as a short redirect. If the agent guidance changes, edit
[`AGENTS.md`](AGENTS.md), not this file; if the documentation index changes,
update [`docs/README.md`](docs/README.md). Preserve the `AGENTS.md` reference so
the repository sync check continues to recognize the redirect.
