# Releases and automatic updates

Every push to `main` runs `.github/workflows/release.yml`. The workflow builds
Windows x64, Linux x64, and macOS Apple Silicon/Intel packages, publishes them
under GitHub Releases, and uploads the signed `latest.json` consumed by the
application updater.

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
