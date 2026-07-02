export interface ColorResult {
  // Primary display, also what gets copied by default
  label: string
  sublabel: string
  // CSS color string for the swatch
  color: string
  // Value copied to the clipboard on Enter
  copyValue: string
}

interface Rgb {
  r: number
  g: number
  b: number
  a: number
}

function clampByte(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)))
}

function clamp01(n: number): number {
  return Math.max(0, Math.min(1, n))
}

function hexToRgb(hex: string): Rgb | null {
  const normalized = hex.replace(/^#/, '')
  if (!/^[0-9a-fA-F]{3,8}$/.test(normalized)) return null
  let full: string
  if (normalized.length === 3 || normalized.length === 4) {
    full = normalized.split('').map(c => c + c).join('')
  } else if (normalized.length === 6 || normalized.length === 8) {
    full = normalized
  } else {
    return null
  }
  const r = parseInt(full.slice(0, 2), 16)
  const g = parseInt(full.slice(2, 4), 16)
  const b = parseInt(full.slice(4, 6), 16)
  const a = full.length === 8 ? parseInt(full.slice(6, 8), 16) / 255 : 1
  if ([r, g, b, a].some(Number.isNaN)) return null
  return { r, g, b, a }
}

function rgbStringToRgb(input: string): Rgb | null {
  const m = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/i.exec(input)
  if (!m) return null
  const r = parseFloat(m[1])
  const g = parseFloat(m[2])
  const b = parseFloat(m[3])
  const a = m[4] === undefined ? 1 : parseFloat(m[4])
  if ([r, g, b, a].some(Number.isNaN)) return null
  return { r: clampByte(r), g: clampByte(g), b: clampByte(b), a: clamp01(a) }
}

function hslStringToRgb(input: string): Rgb | null {
  const m = /^hsla?\(\s*([\d.]+)\s*,\s*([\d.]+)%\s*,\s*([\d.]+)%\s*(?:,\s*([\d.]+)\s*)?\)$/i.exec(input)
  if (!m) return null
  const h = parseFloat(m[1])
  const s = parseFloat(m[2]) / 100
  const l = parseFloat(m[3]) / 100
  const a = m[4] === undefined ? 1 : parseFloat(m[4])
  if ([h, s, l, a].some(Number.isNaN)) return null
  const c = (1 - Math.abs(2 * l - 1)) * s
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1))
  const m1 = l - c / 2
  let r1 = 0, g1 = 0, b1 = 0
  if (h < 60) { r1 = c; g1 = x }
  else if (h < 120) { r1 = x; g1 = c }
  else if (h < 180) { g1 = c; b1 = x }
  else if (h < 240) { g1 = x; b1 = c }
  else if (h < 300) { r1 = x; b1 = c }
  else { r1 = c; b1 = x }
  return {
    r: clampByte((r1 + m1) * 255),
    g: clampByte((g1 + m1) * 255),
    b: clampByte((b1 + m1) * 255),
    a: clamp01(a),
  }
}

function parseColor(input: string): Rgb | null {
  const trimmed = input.trim()
  if (trimmed.startsWith('#')) return hexToRgb(trimmed)
  if (/^rgb/i.test(trimmed)) return rgbStringToRgb(trimmed)
  if (/^hsl/i.test(trimmed)) return hslStringToRgb(trimmed)
  // Bare hex without the leading # (e.g. "3b82f6")
  if (/^[0-9a-fA-F]{6}$/.test(trimmed)) return hexToRgb(`#${trimmed}`)
  return null
}

function rgbToHex({ r, g, b, a }: Rgb): string {
  const toHex = (n: number) => n.toString(16).padStart(2, '0')
  if (a < 1) return `#${toHex(r)}${toHex(g)}${toHex(b)}${toHex(Math.round(a * 255))}`
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`
}

function rgbToRgbString(c: Rgb): string {
  if (c.a < 1) return `rgba(${c.r}, ${c.g}, ${c.b}, ${c.a.toFixed(2)})`
  return `rgb(${c.r}, ${c.g}, ${c.b})`
}

function rgbToHslString(c: Rgb): string {
  const r = c.r / 255
  const g = c.g / 255
  const b = c.b / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  let h = 0
  let s = 0
  if (max !== min) {
    const d = max - min
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
    switch (max) {
      case r: h = ((g - b) / d + (g < b ? 6 : 0)) * 60; break
      case g: h = ((b - r) / d + 2) * 60; break
      case b: h = ((r - g) / d + 4) * 60; break
    }
  }
  const hVal = Math.round(h)
  const sVal = Math.round(s * 100)
  const lVal = Math.round(l * 100)
  if (c.a < 1) return `hsla(${hVal}, ${sVal}%, ${lVal}%, ${c.a.toFixed(2)})`
  return `hsl(${hVal}, ${sVal}%, ${lVal}%)`
}

export function tryColor(input: string): ColorResult | null {
  const c = parseColor(input)
  if (!c) return null
  const hex = rgbToHex(c)
  return {
    label: hex,
    sublabel: `${rgbToRgbString(c)} · ${rgbToHslString(c)}`,
    color: hex,
    copyValue: hex,
  }
}
