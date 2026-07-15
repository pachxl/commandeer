#!/usr/bin/env node
// Pre-commit consistency check: keeps the agent config from drifting apart.
//
// .agents/ is the canonical home for skills + hooks. Claude Code discovers
// skills from .claude/skills/, so the ship-change SKILL.md is mirrored there
// for discovery. This script verifies the mirror still matches the canonical
// copy and that the tool wiring (settings/hooks) still points at .agents/,
// so a contributor can't accidentally re-diverge them. Run by .husky/pre-commit.

import { readFileSync } from 'node:fs'

const root = new URL('../../', import.meta.url)
const read = p => readFileSync(new URL(p, root), 'utf8')

let failed = false
const fail = msg => {
  console.error(`check-agent-sync: ${msg}`)
  failed = true
}

// 1. The Claude-discovery copy of the skill must match the canonical .agents copy.
try {
  const canon = read('.agents/skills/ship-change/SKILL.md')
  const mirror = read('.claude/skills/ship-change/SKILL.md')
  if (canon !== mirror) {
    fail(
      '.claude/skills/ship-change/SKILL.md differs from .agents/skills/ship-change/SKILL.md. ' +
        'Edit the .agents/ copy, then copy it over the .claude/ mirror (they must be byte-identical).',
    )
  }
} catch (e) {
  fail(`could not compare skill copies: ${e.message}`)
}

// 2. CLAUDE.md must redirect to AGENTS.md (single source of truth).
try {
  const claude = read('CLAUDE.md')
  if (!/AGENTS\.md/.test(claude)) {
    fail('CLAUDE.md no longer points at AGENTS.md — keep it as a redirect.')
  }
} catch (e) {
  fail(`could not read CLAUDE.md: ${e.message}`)
}

// 3. Both tools' configs must wire the Stop hook to the shared .agents hook.
for (const [cfg, needle] of [
  ['.claude/settings.json', '.agents/hooks/ship-reminder.mjs'],
  ['.codex/hooks.json', '.agents/hooks/ship-reminder.mjs'],
]) {
  try {
    const txt = read(cfg)
    if (!txt.includes(needle)) {
      fail(`${cfg} must invoke the shared hook at ${needle}.`)
    }
  } catch (e) {
    fail(`could not read ${cfg}: ${e.message}`)
  }
}

if (failed) {
  console.error('\ncheck-agent-sync: agent config is out of sync. Fix the above before committing.')
  process.exit(1)
}
process.exit(0)
