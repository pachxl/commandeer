// Time zone conversion for the @time prefix and the Tools converter.
//
// Grammar:  [time] [source-zone] to <target-zone>
//   "4pm bst to est"      → 4:00 PM in London time, shown in Eastern time
//   "16:30 to tokyo"      → 16:30 in the LOCAL zone, shown in Tokyo time
//   "pst to gmt"          → the current moment, Pacific wall clock vs GMT
//
// Zones resolve from common abbreviations, city names, or any IANA id
// (e.g. america/new_york). All conversions go through Intl, so DST is
// handled by the platform's tz database.

// Abbreviation/city → IANA zone. Ambiguous abbreviations pick the most
// common reading (IST = India, CST = US Central, AST = Atlantic).
const ZONES: Record<string, string> = {
  // UTC & Europe
  utc: 'UTC', gmt: 'UTC', z: 'UTC', zulu: 'UTC',
  bst: 'Europe/London', wet: 'Europe/Lisbon', west: 'Europe/Lisbon',
  cet: 'Europe/Paris', cest: 'Europe/Paris',
  eet: 'Europe/Athens', eest: 'Europe/Athens',
  msk: 'Europe/Moscow', trt: 'Europe/Istanbul',
  // North America
  est: 'America/New_York', edt: 'America/New_York', et: 'America/New_York',
  cst: 'America/Chicago', cdt: 'America/Chicago', ct: 'America/Chicago',
  mst: 'America/Denver', mdt: 'America/Denver', mt: 'America/Denver',
  pst: 'America/Los_Angeles', pdt: 'America/Los_Angeles', pt: 'America/Los_Angeles',
  akst: 'America/Anchorage', akdt: 'America/Anchorage',
  hst: 'Pacific/Honolulu', ast: 'America/Halifax', nst: 'America/St_Johns',
  // South America
  brt: 'America/Sao_Paulo', art: 'America/Argentina/Buenos_Aires',
  clt: 'America/Santiago', cot: 'America/Bogota', pet: 'America/Lima',
  // Africa & Middle East
  sast: 'Africa/Johannesburg', wat: 'Africa/Lagos', cat: 'Africa/Harare',
  eat: 'Africa/Nairobi', gst: 'Asia/Dubai', idt: 'Asia/Jerusalem',
  // Asia
  ist: 'Asia/Kolkata', pkt: 'Asia/Karachi', bdt: 'Asia/Dhaka',
  ict: 'Asia/Bangkok', wib: 'Asia/Jakarta', sgt: 'Asia/Singapore',
  myt: 'Asia/Kuala_Lumpur', hkt: 'Asia/Hong_Kong', pht: 'Asia/Manila',
  jst: 'Asia/Tokyo', kst: 'Asia/Seoul',
  // Oceania
  awst: 'Australia/Perth', acst: 'Australia/Adelaide', acdt: 'Australia/Adelaide',
  aest: 'Australia/Sydney', aedt: 'Australia/Sydney',
  nzst: 'Pacific/Auckland', nzdt: 'Pacific/Auckland',
  // Cities (spaces stripped before lookup, so "new york" → newyork)
  london: 'Europe/London', dublin: 'Europe/Dublin', lisbon: 'Europe/Lisbon',
  paris: 'Europe/Paris', berlin: 'Europe/Berlin', madrid: 'Europe/Madrid',
  rome: 'Europe/Rome', amsterdam: 'Europe/Amsterdam', brussels: 'Europe/Brussels',
  zurich: 'Europe/Zurich', vienna: 'Europe/Vienna', prague: 'Europe/Prague',
  warsaw: 'Europe/Warsaw', stockholm: 'Europe/Stockholm', oslo: 'Europe/Oslo',
  copenhagen: 'Europe/Copenhagen', helsinki: 'Europe/Helsinki', athens: 'Europe/Athens',
  budapest: 'Europe/Budapest', moscow: 'Europe/Moscow', istanbul: 'Europe/Istanbul',
  kyiv: 'Europe/Kyiv', kiev: 'Europe/Kyiv',
  newyork: 'America/New_York', nyc: 'America/New_York', boston: 'America/New_York',
  miami: 'America/New_York', toronto: 'America/Toronto', montreal: 'America/Toronto',
  chicago: 'America/Chicago', dallas: 'America/Chicago', houston: 'America/Chicago',
  denver: 'America/Denver', phoenix: 'America/Phoenix',
  losangeles: 'America/Los_Angeles', la: 'America/Los_Angeles',
  seattle: 'America/Los_Angeles', sanfrancisco: 'America/Los_Angeles', sf: 'America/Los_Angeles',
  vancouver: 'America/Vancouver', anchorage: 'America/Anchorage', honolulu: 'Pacific/Honolulu',
  mexicocity: 'America/Mexico_City', saopaulo: 'America/Sao_Paulo',
  buenosaires: 'America/Argentina/Buenos_Aires', santiago: 'America/Santiago',
  bogota: 'America/Bogota', lima: 'America/Lima',
  cairo: 'Africa/Cairo', lagos: 'Africa/Lagos', nairobi: 'Africa/Nairobi',
  johannesburg: 'Africa/Johannesburg', capetown: 'Africa/Johannesburg',
  dubai: 'Asia/Dubai', riyadh: 'Asia/Riyadh', telaviv: 'Asia/Jerusalem',
  jerusalem: 'Asia/Jerusalem', tehran: 'Asia/Tehran',
  karachi: 'Asia/Karachi', delhi: 'Asia/Kolkata', mumbai: 'Asia/Kolkata',
  kolkata: 'Asia/Kolkata', bangalore: 'Asia/Kolkata', dhaka: 'Asia/Dhaka',
  bangkok: 'Asia/Bangkok', jakarta: 'Asia/Jakarta', singapore: 'Asia/Singapore',
  kualalumpur: 'Asia/Kuala_Lumpur', hongkong: 'Asia/Hong_Kong', manila: 'Asia/Manila',
  beijing: 'Asia/Shanghai', shanghai: 'Asia/Shanghai', taipei: 'Asia/Taipei',
  tokyo: 'Asia/Tokyo', osaka: 'Asia/Tokyo', seoul: 'Asia/Seoul',
  perth: 'Australia/Perth', adelaide: 'Australia/Adelaide', brisbane: 'Australia/Brisbane',
  sydney: 'Australia/Sydney', melbourne: 'Australia/Melbourne', auckland: 'Pacific/Auckland',
  wellington: 'Pacific/Auckland',
}

function isValidIana(zone: string): boolean {
  try {
    new Intl.DateTimeFormat('en-US', { timeZone: zone })
    return true
  } catch {
    return false
  }
}

/// Resolve free text ("bst", "new york", "america/new_york") to an IANA zone.
export function resolveZone(text: string): string | null {
  const key = text.toLowerCase().replace(/[\s_-]+/g, '')
  if (ZONES[key]) return ZONES[key]
  // IANA ids pass through (allow spaces for underscores: "america/new york")
  const iana = text.trim().replace(/\s+/g, '_')
  if (iana.includes('/') && isValidIana(iana)) return iana
  return null
}

interface WallParts {
  y: number
  mo: number
  d: number
  h: number
  mi: number
}

// Wall-clock date/time of `date` as seen in `timeZone`.
function wallParts(date: Date, timeZone: string): WallParts {
  const dtf = new Intl.DateTimeFormat('en-US', {
    timeZone,
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
  })
  const parts: Record<string, string> = {}
  for (const p of dtf.formatToParts(date)) parts[p.type] = p.value
  return { y: +parts.year, mo: +parts.month - 1, d: +parts.day, h: +parts.hour, mi: +parts.minute }
}

// UTC instant for a wall-clock time in `timeZone`. Guess-and-correct twice so
// DST transitions land on the right offset.
function zonedToUtc(p: WallParts, timeZone: string): Date {
  let ts = Date.UTC(p.y, p.mo, p.d, p.h, p.mi)
  for (let i = 0; i < 2; i++) {
    const w = wallParts(new Date(ts), timeZone)
    ts += Date.UTC(p.y, p.mo, p.d, p.h, p.mi) - Date.UTC(w.y, w.mo, w.d, w.h, w.mi)
  }
  return new Date(ts)
}

// "4:00 PM BST"-style rendering of an instant in a zone.
function fmtInZone(date: Date, timeZone: string): string {
  return new Intl.DateTimeFormat('en-US', {
    timeZone,
    hour: 'numeric', minute: '2-digit', hour12: true,
    timeZoneName: 'short',
  }).format(date).replace(/ /g, ' ')
}

// Parse a leading time out of `tokens`, consuming 1–2 tokens.
// Accepts 4pm / 4:30pm / 16:00 / "4 pm" / noon / midnight.
function parseTime(tokens: string[]): { h: number; mi: number; consumed: number } | null {
  const first = tokens[0]
  if (!first) return null
  if (first === 'noon') return { h: 12, mi: 0, consumed: 1 }
  if (first === 'midnight') return { h: 0, mi: 0, consumed: 1 }

  const m = /^(\d{1,2})(?::(\d{2}))?(am|pm)?$/.exec(first)
  if (!m) return null
  let h = parseInt(m[1], 10)
  const mi = m[2] ? parseInt(m[2], 10) : 0
  let meridiem = m[3] as 'am' | 'pm' | undefined
  let consumed = 1
  if (!meridiem && (tokens[1] === 'am' || tokens[1] === 'pm')) {
    meridiem = tokens[1]
    consumed = 2
  }
  if (mi > 59) return null
  if (meridiem) {
    if (h < 1 || h > 12) return null
    if (meridiem === 'pm' && h !== 12) h += 12
    if (meridiem === 'am' && h === 12) h = 0
  } else {
    // Bare "4 bst to est" reads as 4:00; 24h forms like 16:00 also land here
    if (h > 23) return null
  }
  return { h, mi, consumed }
}

export interface TimeResult {
  // The converted time in the target zone, e.g. "11:00 AM EST"
  label: string
  // Full picture: "4:00 PM BST → 11:00 AM EST (next day)"
  sublabel: string
  copy: string
}

/// Convert "…[time] [zone] to <zone>". Returns null when the input doesn't
/// parse as a time conversion (callers treat that as "not a @time query").
export function tryTimeConversion(input: string): TimeResult | null {
  const tokens = input.trim().toLowerCase().split(/\s+/).filter(Boolean)
  const toIdx = tokens.indexOf('to')
  if (toIdx < 0 || toIdx === tokens.length - 1) return null

  const target = resolveZone(tokens.slice(toIdx + 1).join(' '))
  if (!target) return null

  const left = tokens.slice(0, toIdx)
  const time = parseTime(left)
  const zoneText = time ? left.slice(time.consumed).join(' ') : left.join(' ')
  const localZone = Intl.DateTimeFormat().resolvedOptions().timeZone
  const source = zoneText ? resolveZone(zoneText) : localZone
  if (!source) return null

  const now = new Date()
  let instant: Date
  if (time) {
    // "Today" in the source zone at the given wall time
    const today = wallParts(now, source)
    instant = zonedToUtc({ ...today, h: time.h, mi: time.mi }, source)
  } else {
    instant = now
  }

  const sourceText = fmtInZone(instant, source)
  const targetText = fmtInZone(instant, target)

  // Note a day rollover between the two wall clocks
  const sw = wallParts(instant, source)
  const tw = wallParts(instant, target)
  const dayDelta = Date.UTC(tw.y, tw.mo, tw.d) - Date.UTC(sw.y, sw.mo, sw.d)
  const daySuffix = dayDelta > 0 ? ' (next day)' : dayDelta < 0 ? ' (previous day)' : ''

  return {
    label: targetText,
    sublabel: `${sourceText} → ${targetText}${daySuffix}`,
    copy: targetText,
  }
}
