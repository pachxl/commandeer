# Releases and automatic updates

Every push to `main` runs `.github/workflows/release.yml`. The workflow builds
Windows x64, Linux x64, and macOS Apple Silicon/Intel packages, publishes them
under GitHub Releases, and uploads the signed `latest.json` consumed by the
application updater. Publishing does not begin until a dedicated Linux quality
job passes the repository's formatting, agent-sync, frontend build/lint/tests,
and Rust test/Clippy checks. The platform matrix then builds from that same
commit while preserving the independently resolved release version.

`tauri.cjs` is the single build-version resolver. CI supplies
`RELEASE_VERSION`; local builds use an exact numeric Git tag when HEAD is tagged,
then fall back to `package.json` for untagged development commits. All Tauri
commands run through the package script or `release.cjs`; do not invoke the
binary under `node_modules/.bin` directly for a release build.

The workflow's first run publishes tag `1.0.0`. Later runs automatically advance
the patch component (`1.0.1`, `1.0.2`, and so on) using GitHub's persistent
workflow run number. Update `RELEASE_SERIES` in the workflow when starting a new
major or minor line.

## Required repository secret

`TAURI_SIGNING_PRIVATE_KEY` contains the private half of the updater signing
key. It must be configured as a GitHub Actions repository secret. The matching
public key is safe to distribute and is embedded in `src-tauri/tauri.conf.json`.

Back up the private key outside GitHub. Losing it means existing installations
cannot verify any future update signed with a replacement key. Never commit it
to this repository. The local maintainer backup convention is
`~/.tauri/commandeer-updater.key` with mode `0600`.

## Runtime behavior

Packaged release builds check the latest GitHub Release 30 seconds after launch
and every six hours afterwards. A newer SemVer package is downloaded, signature
verified, installed, and Commandeer is restarted. Development builds do not
check for or install published updates.

Optimized binaries launched directly from Cargo's `target/debug` or
`target/release` directories also skip updates. Installed macOS app bundles,
Linux AppImages/system packages, and Windows installer locations remain eligible.

## Keeping this document current

Update this document whenever the release workflow, artifact names, versioning
scheme, signing key, updater endpoint, check cadence, supported targets, or
restart behavior changes. Verify the claims against
`.github/workflows/release.yml`, `src-tauri/tauri.conf.json`,
`src-tauri/src/commands/updater.rs`, and `release.cjs`; never document a secret
value. If signing or updater key material changes, record the migration plan
before changing the public key or repository secret.
