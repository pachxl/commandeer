#!/usr/bin/env node
// Cross-platform release helper: builds Commandeer and copies the artifact
// into bin/. macOS gets the full .app bundle (so deep links and Info.plist
// are preserved); Windows/Linux get the raw binary, matching the previous
// Windows-only behaviour.

const { execSync } = require('child_process')
const fs = require('fs')
const path = require('path')

const platform = process.platform

function run(cmd) {
  console.log(`> ${cmd}`)
  execSync(cmd, { stdio: 'inherit' })
}

function copy(src, dst) {
  fs.mkdirSync(path.dirname(dst), { recursive: true })
  fs.copyFileSync(src, dst)
  console.log(`Copied ${src} -> ${dst}`)
}

function copyDir(src, dst) {
  fs.rmSync(dst, { recursive: true, force: true })
  fs.mkdirSync(path.dirname(dst), { recursive: true })
  fs.cpSync(src, dst, { recursive: true, preserveTimestamps: true })
  console.log(`Copied ${src} -> ${dst}`)
}

if (platform === 'darwin') {
  // Build the .app bundle so it has the correct Info.plist, icon, and deep-link
  // scheme. We target only the 'app' bundle because .dmg creation requires
  // external tooling (create-dmg) that may not be installed; the .app is the
  // usable artifact anyway.
  run('tauri build --bundles app')
  copyDir(
    'src-tauri/target/release/bundle/macos/commandeer.app',
    'bin/commandeer.app'
  )
} else if (platform === 'win32') {
  run('tauri build --no-bundle')
  copy('src-tauri/target/release/commandeer.exe', 'bin/commandeer.exe')
} else {
  run('tauri build --no-bundle')
  copy('src-tauri/target/release/commandeer', 'bin/commandeer')
}
