# Script commands

Scripts are user-owned commands discovered from `AppConfig.scripts_dir` by
`src-tauri/src/commands/fs.rs`. The frontend converts `ScriptInfo` records into
palette commands in `src/commands/index.ts`.

## Files that are discovered

| Platform | Extensions / rule                                                    |
| -------- | -------------------------------------------------------------------- |
| Windows  | `.bat`, `.cmd`, `.ps1`, `.lnk`, and `.code-workspace`                |
| Linux    | `.sh`, `.desktop`, `.code-workspace`, `.AppImage`, or executable bit |
| macOS    | `.sh`, `.command`, `.code-workspace`, or executable bit              |

Subdirectories become virtual folders. A sibling PNG named after a script or
folder supplies an icon; named metadata icons use the built-in icon library
first. Windows `.lnk` files can fall back to shell icon extraction. Linux
`.desktop` files can provide their declared name and icon.

## Header metadata

The scanner reads up to 8192 bytes and accepts `//`, `--`, `#`, or `;` comment
markers. Both `@raycast.*` and `@vicinae.*` prefixes are accepted.

| Directive                | Value                                                   | Effect                                        |
| ------------------------ | ------------------------------------------------------- | --------------------------------------------- |
| `title`                  | string                                                  | Display label instead of filename             |
| `description`            | string                                                  | Row description/sublabel                      |
| `icon` / `iconDark`      | built-in icon name                                      | Named icon lookup                             |
| `mode`                   | `fullOutput`, `compact`, `inline`, `silent`, `terminal` | Accessory badge and execution/display mode    |
| `keywords`               | JSON string array                                       | Additional fuzzy-search terms                 |
| `needsConfirmation`      | `true`/`false`                                          | Pushes a confirmation step before running     |
| `refreshTime`            | `5s`, `2m`, `1h`, or `1d`                               | Required with `inline`; polls captured stdout |
| `author` / `packageName` | string                                                  | Metadata retained for future presentation     |
| `currentDirectoryPath`   | path                                                    | Script working-directory metadata             |
| `argument1`–`argument3`  | JSON object                                             | Text/password/dropdown argument metadata      |

Arguments are parsed and exposed in `ScriptInfo`; the current command conversion
uses the command/confirmation/live-output paths, so do not assume every parsed
field already has a UI flow.

Example:

```sh
#!/bin/sh
# @vicinae.title Current branch
# @vicinae.description Show the active Git branch
# @vicinae.mode inline
# @vicinae.refreshTime 5s
# @vicinae.keywords ["git", "branch"]
git branch --show-current
```

`inline` plus `refreshTime` creates a live row whose sublabel is captured stdout.
The output is kept out of ranking text so refreshing it does not reorder the
palette. Inline scripts start only after the palette is confirmed focused and
their timers are stopped again while it is hidden.

## Execution and safety

Bare scripts run through the platform launcher path; folders run their selected
child. Confirmation is intentionally decided after fuzzy selection, so a
destructive script must declare `needsConfirmation`. Script errors are returned
to the palette and should not be presented as successful execution.

Use a sibling PNG for a custom image icon, or a recognized built-in icon name in
metadata. Do not put secrets in metadata: files are read from disk and may be
shown in details or cached script records.

## Keeping this document current

Update this page when file discovery rules, metadata directives, argument
semantics, execution modes, icon lookup, confirmation behavior, or live-output
polling changes. Verify against `commands/fs.rs`, `src/commands/index.ts`, and
`src/hooks/useInlineScripts.ts`; add an example or test when a directive is
introduced.
