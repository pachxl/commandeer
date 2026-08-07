#!/usr/bin/env node

const { execFileSync, spawnSync } = require('child_process')
const path = require('path')

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/

function exactGitTag() {
  try {
    return execFileSync('git', ['describe', '--tags', '--exact-match', '--match', '[0-9]*'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return ''
  }
}

function resolveBuildVersion({ releaseVersion = process.env.RELEASE_VERSION, gitTag = exactGitTag() } = {}) {
  const candidate = (releaseVersion || gitTag || require('./package.json').version).replace(/^v/, '')
  if (!SEMVER.test(candidate)) {
    throw new Error(`Invalid Commandeer build version: ${candidate}`)
  }
  return candidate
}

function buildArgs(args, version = resolveBuildVersion()) {
  if (args[0] !== 'build' || args.includes('--config')) return args
  return [...args, '--config', JSON.stringify({ version })]
}

function run() {
  const executable = path.join(__dirname, 'node_modules', '.bin', process.platform === 'win32' ? 'tauri.cmd' : 'tauri')
  const args = buildArgs(process.argv.slice(2))
  const result = spawnSync(executable, args, { stdio: 'inherit', env: process.env })
  if (result.error) throw result.error
  process.exitCode = result.status ?? 1
}

if (require.main === module) run()

module.exports = { buildArgs, resolveBuildVersion }
