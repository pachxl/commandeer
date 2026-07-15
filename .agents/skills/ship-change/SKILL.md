---
name: ship-change
description: >
  Ship a completed change end-to-end in the commandeer repo: commit + push, stop the
  running app, rebuild the release binary, then relaunch it. Invoke this AFTER every finished
  task, bug fix, or feature — whenever a unit of work is complete and verified — so the
  running process always reflects committed code. Triggers on "done", "that's working",
  "ship it", or the natural end of any implementation task.
---

# Ship a completed change

Run this the moment a task, fix, or feature is complete and verified. The goal: the
git remote and the running process both reflect the finished work. Do all four steps
in order; do not stop after committing.

## 1. Commit and push

```bash
git add -A
git status                 # confirm what you're about to commit
git commit -m "<concise subject>

<why, if not obvious from the diff>

Co-Authored-By: <assistant name + model> <noreply@anthropic.com>
<Assistant>-Session: <session URL>"
git push
```

- If on the default branch (`main`), that is fine here — this repo ships from `main`.
- Match the existing commit-message style: imperative subject, a body explaining the
  _why_ for anything non-obvious.
- Attribute the change to whichever AI assistant produced it with the two footer
  lines above — e.g. `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>` +
  `Claude-Session: <url>`, or `Co-Authored-By: Codex Opus 4.8 <noreply@anthropic.com>`
  - `Codex-Session: <url>`. If you authored the change yourself, omit the footers.
- If `git push` is rejected (remote moved), `git pull --rebase` then push again.

## 2. Stop the running process

Stop Commandeer **before** rebuilding. On Windows, the running process locks
`bin\commandeer.exe`, so `npm run release` cannot copy the new executable and fails with
`EBUSY` if this step is delayed until after the build.

```bash
# Linux / macOS
pkill -x commandeer        # -x only: `pkill -f commandeer` also kills the invoking shell
```

```powershell
# Windows
Stop-Process -Name commandeer -Force -ErrorAction SilentlyContinue
```

It is fine if no process is running. Do not relaunch yet.

## 3. Rebuild the release binary

Only a **release** build is representative — the dev build loads `localhost:5173` and is
~15× slower (never judge screenshot/latency behavior from it). Detect the OS and run the
matching commands (in bash: `uname` → `Linux`/`Darwin`; Windows shell has no `uname`).

```bash
# Linux / macOS
source ~/.cargo/env                     # ensure cargo is on PATH
npm run tauri build -- --no-bundle      # bare binary at src-tauri/target/release/commandeer

# Windows (PowerShell)
npm run release                         # build + copy commandeer.exe to bin\
```

If TypeScript is the only concern, `npm run build` (tsc + vite) is the type-check, but a
full restart still needs the Tauri build above.

If the build or artifact copy fails, stop and report it. Do not relaunch a stale binary or
claim the change shipped.

## 4. Relaunch the new release

**Linux / macOS:**

```bash
nohup ./src-tauri/target/release/commandeer >/dev/null 2>&1 &
```

**Windows (PowerShell):**

```powershell
Start-Process .\bin\commandeer.exe        # or the freshly built target\release\commandeer.exe
```

- The single-instance plugin means launching while an instance is alive just **toggles**
  the palette — the process-stop step above is mandatory before this relaunch.
- **Linux/Wayland:** relaunching the binary is also the reliable palette trigger (global
  X11 shortcut grabs don't work). Set `COMMANDEER_NO_AUTOHIDE=1` to keep the window up for
  inspection.
- **macOS:** the bare binary path matches Linux. If you built a `.app` bundle instead, use
  `open -a Commandeer` (or `open path/to/Commandeer.app`).
- **Windows:** launch only after `npm run release` confirms the copy into `bin\` succeeded.

## Done

Report: the commit hash pushed, that the release build succeeded, and that the app was
restarted on the new binary. If any step fails (build error, push rejected), stop and
surface it — do not claim the change is shipped.
