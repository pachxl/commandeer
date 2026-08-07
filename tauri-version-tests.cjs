const test = require('node:test')
const assert = require('node:assert/strict')

const { buildArgs, resolveBuildVersion } = require('./tauri.cjs')

test('release environment is the authoritative build version', () => {
  assert.equal(resolveBuildVersion({ releaseVersion: '2.3.4', gitTag: '1.0.14' }), '2.3.4')
})

test('an exact git tag versions local release builds', () => {
  assert.equal(resolveBuildVersion({ releaseVersion: '', gitTag: 'v1.0.14' }), '1.0.14')
})

test('build commands receive a Tauri version override', () => {
  assert.deepEqual(buildArgs(['build', '--no-bundle'], '1.0.14'), [
    'build',
    '--no-bundle',
    '--config',
    '{"version":"1.0.14"}',
  ])
})

test('dev commands and explicit configs are preserved', () => {
  assert.deepEqual(buildArgs(['dev'], '1.0.14'), ['dev'])
  assert.deepEqual(buildArgs(['build', '--config', 'custom.json'], '1.0.14'), ['build', '--config', 'custom.json'])
})
