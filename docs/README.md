# Commandeer documentation

This directory is the maintainer map for Commandeer. It explains how the
frontend, Tauri bridge, Rust services, platform integrations, storage, release
process, and test strategy fit together. The source code remains authoritative;
each page links to the modules that implement the behavior it describes.

## Start here

| Need                                          | Read                                                                    |
| --------------------------------------------- | ----------------------------------------------------------------------- |
| Understand the app as a whole                 | [`architecture.md`](architecture.md)                                    |
| Add or change a palette command               | [`frontend.md`](frontend.md) and [`commands.md`](commands.md)           |
| Add or change a Rust feature or IPC command   | [`backend.md`](backend.md)                                              |
| Change settings, defaults, or persistence     | [`configuration.md`](configuration.md) and [`storage.md`](storage.md)   |
| Work on Windows, Linux, or macOS behavior     | [`platforms.md`](platforms.md)                                          |
| Understand a major feature                    | [`features.md`](features.md)                                            |
| Add tests or verify a change                  | [`testing.md`](testing.md)                                              |
| Diagnose a local or platform-specific failure | [`troubleshooting.md`](troubleshooting.md)                              |
| Find the owner of a source file               | [`source-map.md`](source-map.md)                                        |
| Ship a release                                | [`../RELEASING.md`](../RELEASING.md) and [`../AGENTS.md`](../AGENTS.md) |

## Documentation conventions

- Link behavior to the source module that owns it. Do not describe an invariant
  without naming where it is enforced.
- Label platform support as implemented, compile-only, manually verified, or
  intentionally unsupported. “Works on Unix” is not precise enough for this
  project.
- Document defaults, paths, permissions, fallback chains, and failure behavior;
  these are the details most likely to be lost during maintenance.
- Keep user instructions in `README.md`, repository-wide agent/build policy in
  `AGENTS.md`, release mechanics in `RELEASING.md`, and subsystem detail here.
- When a source file is not important enough for its own page, give it an entry
  in [`source-map.md`](source-map.md). Every frontend and Rust module should
  have an identified documentation owner.

## Keeping this documentation current

Every page in this directory ends with a maintenance section. When a change
touches a documented interface, update the relevant page in the same commit and
check the links in this index. When adding a new subsystem page, add it to the
table above and add its source files to [`source-map.md`](source-map.md).
Before committing documentation-only changes, run `npm run format:check`; for
code-adjacent changes, follow [`testing.md`](testing.md).
