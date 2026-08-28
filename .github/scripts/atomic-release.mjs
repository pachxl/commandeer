import { appendFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const API_ROOT = 'https://api.github.com'
const API_VERSION = '2022-11-28'
const LATEST_JSON = 'latest.json'
const RELEASE_WORKFLOW = 'release.yml'

export const UPDATER_PLATFORM_KEYS = [
  'darwin-aarch64',
  'darwin-aarch64-app',
  'darwin-x86_64',
  'darwin-x86_64-app',
  'linux-x86_64',
  'linux-x86_64-appimage',
  'linux-x86_64-deb',
  'windows-x86_64',
  'windows-x86_64-nsis',
]

function updaterSpecs(version) {
  const prefix = `commandeer-${version}`

  return [
    {
      assetName: `${prefix}-darwin-aarch64.app.tar.gz`,
      platformKeys: ['darwin-aarch64', 'darwin-aarch64-app'],
    },
    {
      assetName: `${prefix}-darwin-x64.app.tar.gz`,
      platformKeys: ['darwin-x86_64', 'darwin-x86_64-app'],
    },
    {
      assetName: `${prefix}-linux-amd64.AppImage`,
      platformKeys: ['linux-x86_64', 'linux-x86_64-appimage'],
    },
    {
      assetName: `${prefix}-linux-amd64.deb`,
      platformKeys: ['linux-x86_64-deb'],
    },
    {
      assetName: `${prefix}-windows-x64-setup.exe`,
      platformKeys: ['windows-x86_64', 'windows-x86_64-nsis'],
    },
  ].map(spec => ({ ...spec, signatureName: `${spec.assetName}.sig` }))
}

export function requiredReleaseAssetNames(version) {
  const prefix = `commandeer-${version}`
  return [
    ...updaterSpecs(version).flatMap(spec => [spec.assetName, spec.signatureName]),
    `${prefix}-darwin-aarch64.dmg`,
    `${prefix}-darwin-x64.dmg`,
  ]
}

function assetMapForVersion(version, assets) {
  const byName = new Map()
  for (const asset of assets) {
    if (byName.has(asset.name)) throw new Error(`Duplicate release asset: ${asset.name}`)
    byName.set(asset.name, asset)
  }

  const missing = requiredReleaseAssetNames(version).filter(name => {
    const asset = byName.get(name)
    return !asset || !Number.isFinite(asset.size) || asset.size <= 0
  })
  if (missing.length > 0) throw new Error(`Missing or empty release assets: ${missing.join(', ')}`)

  return byName
}

function validateSignature(name, signature) {
  const trimmed = signature.trim()
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(trimmed)) throw new Error(`Invalid updater signature: ${name}`)

  const decoded = Buffer.from(trimmed, 'base64').toString('utf8')
  if (!decoded.includes('untrusted comment: signature from tauri secret key')) {
    throw new Error(`Unexpected updater signature payload: ${name}`)
  }
  return trimmed
}

export async function buildUpdaterManifest({ version, assets, readSignature, notes = '', pubDate }) {
  const byName = assetMapForVersion(version, assets)
  const platforms = {}

  for (const spec of updaterSpecs(version)) {
    const asset = byName.get(spec.assetName)
    const signatureAsset = byName.get(spec.signatureName)
    const signature = validateSignature(spec.signatureName, await readSignature(signatureAsset))

    for (const platformKey of spec.platformKeys) {
      platforms[platformKey] = { signature, url: asset.url }
    }
  }

  const actualKeys = Object.keys(platforms).sort()
  const expectedKeys = [...UPDATER_PLATFORM_KEYS].sort()
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    throw new Error(`Updater manifest platform mismatch: ${actualKeys.join(', ')}`)
  }

  return {
    version,
    notes,
    pub_date: pubDate ?? new Date().toISOString(),
    platforms,
  }
}

function requiredEnv(name) {
  const value = process.env[name]
  if (!value) throw new Error(`${name} is required`)
  return value
}

function repositoryPath() {
  return `/repos/${requiredEnv('GITHUB_REPOSITORY')}`
}

async function githubRequest(pathOrUrl, { method = 'GET', body, rawBody, accept, contentType, raw = false } = {}) {
  const url = pathOrUrl.startsWith('http') ? pathOrUrl : `${API_ROOT}${pathOrUrl}`
  const headers = {
    Accept: accept ?? 'application/vnd.github+json',
    Authorization: `Bearer ${requiredEnv('GITHUB_TOKEN')}`,
    'User-Agent': 'commandeer-atomic-release',
    'X-GitHub-Api-Version': API_VERSION,
  }
  if (contentType) headers['Content-Type'] = contentType

  const response = await fetch(url, {
    method,
    headers,
    body: rawBody ?? (body === undefined ? undefined : JSON.stringify(body)),
  })
  if (!response.ok) {
    const details = await response.text()
    throw new Error(`${method} ${url} failed (${response.status}): ${details}`)
  }
  if (raw) return response

  const text = await response.text()
  return text ? JSON.parse(text) : undefined
}

async function listAll(path) {
  const separator = path.includes('?') ? '&' : '?'
  const results = []
  for (let page = 1; ; page += 1) {
    const batch = await githubRequest(`${path}${separator}per_page=100&page=${page}`)
    results.push(...batch)
    if (batch.length < 100) return results
  }
}

async function prepareRelease() {
  const version = requiredEnv('RELEASE_VERSION')
  const targetCommitish = requiredEnv('RELEASE_TARGET_SHA')
  const releases = await listAll(`${repositoryPath()}/releases`)
  const matches = releases.filter(release => release.tag_name === version)

  if (matches.length > 1) throw new Error(`Multiple releases already use tag ${version}`)

  let release = matches[0]
  if (release) {
    if (!release.draft) throw new Error(`Release ${version} is already published`)
    if (release.target_commitish !== targetCommitish) {
      throw new Error(`Draft ${version} targets ${release.target_commitish}, expected ${targetCommitish}`)
    }
    console.log(`Reusing draft release ${version} (${release.id})`)
  } else {
    release = await githubRequest(`${repositoryPath()}/releases`, {
      method: 'POST',
      body: {
        tag_name: version,
        target_commitish: targetCommitish,
        name: `Commandeer ${version}`,
        draft: true,
        prerelease: false,
        generate_release_notes: true,
      },
    })
    console.log(`Created draft release ${version} (${release.id})`)
  }

  appendFileSync(requiredEnv('GITHUB_OUTPUT'), `release_id=${release.id}\n`)
}

async function waitForEarlierRuns() {
  const currentRunNumber = Number(requiredEnv('GITHUB_RUN_NUMBER'))
  if (!Number.isSafeInteger(currentRunNumber)) throw new Error('GITHUB_RUN_NUMBER must be an integer')

  const path = `${repositoryPath()}/actions/workflows/${RELEASE_WORKFLOW}/runs?exclude_pull_requests=true&per_page=100`
  const deadline = Date.now() + 5 * 60 * 60 * 1000

  for (;;) {
    const { workflow_runs: runs } = await githubRequest(path)
    const blockers = runs.filter(run => run.run_number < currentRunNumber && run.status !== 'completed')
    if (blockers.length === 0) return
    if (Date.now() >= deadline) {
      throw new Error(`Timed out waiting for earlier release runs: ${blockers.map(run => run.run_number).join(', ')}`)
    }

    console.log(`Waiting for earlier release runs: ${blockers.map(run => run.run_number).join(', ')}`)
    await new Promise(resolve => setTimeout(resolve, 30_000))
  }
}

async function readAsset(asset) {
  const response = await githubRequest(asset.url, { accept: 'application/octet-stream', raw: true })
  return response.text()
}

function validateUploadedManifest(actual, expected) {
  if (actual.version !== expected.version || actual.notes !== expected.notes || actual.pub_date !== expected.pub_date) {
    throw new Error('Uploaded latest.json metadata does not match the generated manifest')
  }

  const actualKeys = Object.keys(actual.platforms ?? {}).sort()
  const expectedKeys = [...UPDATER_PLATFORM_KEYS].sort()
  if (JSON.stringify(actualKeys) !== JSON.stringify(expectedKeys)) {
    throw new Error(`Uploaded latest.json platform mismatch: ${actualKeys.join(', ')}`)
  }

  for (const key of UPDATER_PLATFORM_KEYS) {
    const actualPlatform = actual.platforms[key]
    const expectedPlatform = expected.platforms[key]
    if (actualPlatform.url !== expectedPlatform.url || actualPlatform.signature !== expectedPlatform.signature) {
      throw new Error(`Uploaded latest.json entry does not match for ${key}`)
    }
  }
}

async function finalizeRelease() {
  const version = requiredEnv('RELEASE_VERSION')
  const targetCommitish = requiredEnv('RELEASE_TARGET_SHA')
  const releaseId = requiredEnv('RELEASE_ID')

  await waitForEarlierRuns()

  const releasePath = `${repositoryPath()}/releases/${releaseId}`
  const release = await githubRequest(releasePath)
  if (!release.draft) throw new Error(`Release ${version} is not a draft`)
  if (release.tag_name !== version) throw new Error(`Release tag is ${release.tag_name}, expected ${version}`)
  if (release.target_commitish !== targetCommitish) {
    throw new Error(`Release targets ${release.target_commitish}, expected ${targetCommitish}`)
  }

  const assetsPath = `${releasePath}/assets`
  let assets = await listAll(assetsPath)
  const manifest = await buildUpdaterManifest({
    version,
    assets,
    readSignature: readAsset,
    notes: release.body ?? '',
  })

  const previousManifest = assets.find(asset => asset.name === LATEST_JSON)
  if (previousManifest) {
    await githubRequest(`${repositoryPath()}/releases/assets/${previousManifest.id}`, { method: 'DELETE' })
  }

  const uploadUrl = new URL(release.upload_url.split('{')[0])
  uploadUrl.searchParams.set('name', LATEST_JSON)
  const uploadedManifest = await githubRequest(uploadUrl.toString(), {
    method: 'POST',
    rawBody: Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`),
    contentType: 'application/json',
  })

  const uploadedText = await readAsset(uploadedManifest)
  validateUploadedManifest(JSON.parse(uploadedText), manifest)

  assets = await listAll(assetsPath)
  if (!assets.some(asset => asset.name === LATEST_JSON && asset.size > 0)) {
    throw new Error('latest.json was not present after upload')
  }

  await githubRequest(releasePath, {
    method: 'PATCH',
    body: { draft: false, prerelease: false, make_latest: 'true' },
  })
  console.log(`Published Commandeer ${version} with a complete updater manifest`)
}

async function main() {
  const command = process.argv[2]
  if (command === 'prepare') return prepareRelease()
  if (command === 'finalize') return finalizeRelease()
  throw new Error('Usage: node .github/scripts/atomic-release.mjs <prepare|finalize>')
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch(error => {
    console.error(error instanceof Error ? error.message : error)
    process.exitCode = 1
  })
}
