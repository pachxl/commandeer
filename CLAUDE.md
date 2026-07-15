# CLAUDE.md

This repository keeps a **single source of truth** for agent and contributor
guidance: [AGENTS.md](AGENTS.md).

See AGENTS.md for the project scope, code style, build/deploy process, the
ship-change workflow, and how skills/hooks are wired for both Claude Code and
Codex. This file is intentionally a redirect only — do not duplicate AGENTS.md
content here. A pre-commit check (`.agents/hooks/check-agent-sync.mjs`) enforces
that this file keeps pointing at AGENTS.md.
