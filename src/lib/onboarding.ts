export const ONBOARDING_VERSION_KEY = 'commandeer:onboarding-version'
export const CURRENT_ONBOARDING_VERSION = '1'

// These keys predate onboarding. Their presence means this is an established
// installation, so a newly introduced welcome flow should not interrupt it.
const EXISTING_INSTALL_KEYS = [
  'commandeer:scripts',
  'commandeer:last',
  'commandeer:gamemode',
  'commandeer:system-stats-visible',
]

export function shouldShowOnboarding(read: (key: string) => string | null): boolean {
  if (read(ONBOARDING_VERSION_KEY) === CURRENT_ONBOARDING_VERSION) return false
  return !EXISTING_INSTALL_KEYS.some(key => read(key) !== null)
}

export function markOnboardingSeen(write: (key: string, value: string) => void): void {
  write(ONBOARDING_VERSION_KEY, CURRENT_ONBOARDING_VERSION)
}
