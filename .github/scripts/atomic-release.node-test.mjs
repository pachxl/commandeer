import assert from 'node:assert/strict'
import test from 'node:test'

// Named `node-test` so Vitest does not also collect this Node-native suite.
import { UPDATER_PLATFORM_KEYS, buildUpdaterManifest, requiredReleaseAssetNames } from './atomic-release.mjs'

const version = '1.2.3'
const signature = Buffer.from('untrusted comment: signature from tauri secret key\nsigned fixture').toString('base64')
const expectedAssetNames = [
  'commandeer-1.2.3-darwin-aarch64.app.tar.gz',
  'commandeer-1.2.3-darwin-aarch64.app.tar.gz.sig',
  'commandeer-1.2.3-darwin-x64.app.tar.gz',
  'commandeer-1.2.3-darwin-x64.app.tar.gz.sig',
  'commandeer-1.2.3-linux-amd64.AppImage',
  'commandeer-1.2.3-linux-amd64.AppImage.sig',
  'commandeer-1.2.3-linux-amd64.deb',
  'commandeer-1.2.3-linux-amd64.deb.sig',
  'commandeer-1.2.3-windows-x64-setup.exe',
  'commandeer-1.2.3-windows-x64-setup.exe.sig',
  'commandeer-1.2.3-darwin-aarch64.dmg',
  'commandeer-1.2.3-darwin-x64.dmg',
]

function completeAssets() {
  return expectedAssetNames.map((name, id) => ({
    id,
    name,
    size: 100,
    url: `https://api.github.test/releases/assets/${id}`,
  }))
}

test('builds the complete nine-key updater manifest', async () => {
  assert.deepEqual(requiredReleaseAssetNames(version), expectedAssetNames)

  const manifest = await buildUpdaterManifest({
    version,
    assets: completeAssets(),
    readSignature: async () => signature,
    notes: 'Release notes',
    pubDate: '2026-08-28T12:00:00.000Z',
  })

  assert.deepEqual(Object.keys(manifest.platforms).sort(), [...UPDATER_PLATFORM_KEYS].sort())
  assert.deepEqual(manifest.platforms['darwin-aarch64'], manifest.platforms['darwin-aarch64-app'])
  assert.deepEqual(manifest.platforms['linux-x86_64'], manifest.platforms['linux-x86_64-appimage'])
  assert.deepEqual(manifest.platforms['windows-x86_64'], manifest.platforms['windows-x86_64-nsis'])
})

test('rejects a release with a missing updater payload', async () => {
  const assets = completeAssets().filter(asset => !asset.name.endsWith('-windows-x64-setup.exe'))

  await assert.rejects(
    buildUpdaterManifest({ version, assets, readSignature: async () => signature }),
    /Missing or empty release assets: .*windows-x64-setup\.exe/,
  )
})
