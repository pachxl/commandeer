#!/usr/bin/env node
// Stop hook: nudge — but never force — shipping a completed change.
//
// Cross-platform (Windows / macOS / Linux): written in Node (always present in
// this npm/Tauri project) and invoked as `node .claude/hooks/ship-reminder.mjs`,
// a command line that parses identically in bash, PowerShell, and cmd. No shell
// builtins, no $VAR expansion, no bash-isms — so it behaves the same on all three.
//
// Design goal (per user): do NOT commit on every stop, and do not treat every
// changed file as a shippable unit. This hook only *reminds*; the model decides
// whether the uncommitted work is actually a complete, verified feature/fix.
//
// Behavior:
//   - No uncommitted changes          -> allow stop silently (nothing to ship).
//   - Already re-invoked by this hook  -> allow stop (stop_hook_active guard;
//                                         prevents nag loops / forced commits).
//   - Uncommitted changes present      -> block once, feed a reminder back to
//                                         the model to judge + ship-or-leave.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

let input = "";
try {
  input = readFileSync(0, "utf8");
} catch {
  input = "";
}

let payload = {};
try {
  payload = JSON.parse(input || "{}");
} catch {
  payload = {};
}

// Don't re-fire if this stop was itself triggered by a Stop-hook continuation.
if (payload.stop_hook_active === true) process.exit(0);

function git(args) {
  // execFileSync (no shell) — argument handling is identical on every OS.
  return execFileSync("git", args, { encoding: "utf8" });
}

// Only meaningful inside a git work tree.
try {
  git(["rev-parse", "--is-inside-work-tree"]);
} catch {
  process.exit(0);
}

let changes = "";
try {
  changes = git(["status", "--short"]).trim();
} catch {
  process.exit(0);
}

if (!changes) process.exit(0); // clean tree — nothing to ship, stop normally.

const reason = `You are about to stop with uncommitted changes in the working tree:

${changes}

Decide whether this represents a COMPLETE, verified feature or bug fix:
  - If YES — a coherent unit of work is finished and working — run the
    \`ship-change\` skill now: commit + push, rebuild the release binary, then
    restart the running app.
  - If NO — the work is partial, mid-task, experimental, or unverified — do
    NOT commit. Just say so in one line (work left uncommitted on purpose) and
    stop. Never commit a half-done change or a stray edited file just to
    satisfy this check.

Group only the files that belong to the completed change into the commit; leave
unrelated in-progress edits out of it.`;

process.stdout.write(JSON.stringify({ decision: "block", reason }));
process.exit(0);
