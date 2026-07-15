type Token =
  | { type: 'number'; value: number }
  | { type: 'name'; value: string }
  | { type: 'op'; value: string }
  | { type: 'paren'; value: '(' | ')' }
  | { type: 'eof' }

type ValueToken = Exclude<Token, { type: 'eof' }>

// A value that may be a percent literal (e.g. the `10%` in `80 + 10%`);
// how it combines depends on the surrounding operator.
interface Val {
  v: number
  pct: boolean
}

class Tokenizer {
  private input: string
  private pos = 0
  sawBaseLiteral = false

  constructor(input: string) {
    this.input = input.replace(/\s+/g, ' ')
  }

  next(): Token {
    while (this.pos < this.input.length && this.input[this.pos] === ' ') this.pos++
    if (this.pos >= this.input.length) return { type: 'eof' }
    const ch = this.input[this.pos]

    if (ch === '(' || ch === ')') {
      this.pos++
      return { type: 'paren', value: ch }
    }

    if (/[0-9.]/.test(ch)) {
      // Base literals: 0x…, 0b…, 0o…
      if (ch === '0' && this.pos + 1 < this.input.length) {
        const marker = this.input[this.pos + 1].toLowerCase()
        const digits: Record<string, RegExp> = { x: /[0-9a-fA-F]/, b: /[01]/, o: /[0-7]/ }
        if (digits[marker]) {
          const start = this.pos + 2
          let end = start
          while (end < this.input.length && digits[marker].test(this.input[end])) end++
          if (end > start) {
            this.pos = end
            this.sawBaseLiteral = true
            const radix = marker === 'x' ? 16 : marker === 'b' ? 2 : 8
            return { type: 'number', value: parseInt(this.input.slice(start, end), radix) }
          }
        }
      }
      const start = this.pos
      let dotCount = 0
      while (this.pos < this.input.length && (/[0-9]/.test(this.input[this.pos]) || this.input[this.pos] === '.')) {
        if (this.input[this.pos] === '.') dotCount++
        if (dotCount > 1) break
        this.pos++
      }
      const raw = this.input.slice(start, this.pos)
      const value = parseFloat(raw)
      if (Number.isNaN(value)) throw new Error(`Invalid number: ${raw}`)
      return { type: 'number', value }
    }

    if (/[a-zA-Z]/.test(ch)) {
      const start = this.pos
      while (this.pos < this.input.length && /[a-zA-Z0-9_]/.test(this.input[this.pos])) this.pos++
      return { type: 'name', value: this.input.slice(start, this.pos) }
    }

    if ('+-*/^%,|&<>!'.includes(ch)) {
      this.pos++
      return { type: 'op', value: ch }
    }

    throw new Error(`Unexpected character: ${ch}`)
  }
}

class Parser {
  private tokens: Token[]
  private pos = 0
  sawBaseLiteral = false

  constructor(input: string) {
    const tokenizer = new Tokenizer(input)
    this.tokens = []
    let token: Token
    do {
      token = tokenizer.next()
      this.tokens.push(token)
    } while (token.type !== 'eof')
    this.sawBaseLiteral = tokenizer.sawBaseLiteral
  }

  private current(): Token {
    return this.tokens[this.pos]
  }

  private peek(): ValueToken {
    const token = this.current()
    if (token.type === 'eof') throw new Error('Unexpected end of input')
    return token
  }

  private eat(expected?: string): ValueToken {
    const token = this.peek()
    if (expected && (token.type !== 'op' || token.value !== expected)) {
      throw new Error(`Expected ${expected} but got ${token.value}`)
    }
    this.pos++
    return token
  }

  parse(): number {
    const result = this.expr()
    if (this.current().type !== 'eof') throw new Error('Unexpected token at end')
    // A bare percent expression evaluates to its fraction: `50%` → 0.5
    return result.pct ? result.v / 100 : result.v
  }

  private resolve(val: Val): number {
    return val.pct ? val.v / 100 : val.v
  }

  private expr(): Val {
    let left = this.term()
    while (this.currentOpMatches('+', '-')) {
      const op = this.eat().value
      const right = this.term()
      let v: number
      if (right.pct && !left.pct) {
        // `80 + 10%` → 88, `80 - 10%` → 72
        v = op === '+' ? left.v * (1 + right.v / 100) : left.v * (1 - right.v / 100)
      } else {
        const l = this.resolve(left)
        const r = this.resolve(right)
        v = op === '+' ? l + r : l - r
      }
      left = { v, pct: false }
    }
    return left
  }

  private term(): Val {
    let left = this.factor()
    while (this.currentOpMatches('*', '/', '%') || this.currentNameMatches('of')) {
      if (this.currentNameMatches('of')) {
        // `15% of 80` → 12
        this.pos++
        const right = this.factor()
        left = { v: this.resolve(left) * this.resolve(right), pct: false }
        continue
      }
      const op = this.eat().value
      const right = this.factor()
      const l = this.resolve(left)
      const r = this.resolve(right)
      let v: number
      if (op === '*') v = l * r
      else if (op === '/') v = l / r
      else v = l % r
      left = { v, pct: false }
    }
    return left
  }

  private factor(): Val {
    let value = this.unary()
    while (this.currentOpMatches('^')) {
      this.eat()
      const right = this.unary()
      value = { v: Math.pow(this.resolve(value), this.resolve(right)), pct: false }
    }
    // Postfix percent — only when '%' is not being used as binary modulo
    // (i.e. it is not followed by the start of another operand)
    if (this.currentOpMatches('%') && !this.nextStartsOperand()) {
      this.eat()
      return { v: value.v, pct: true }
    }
    return value
  }

  private nextStartsOperand(): boolean {
    const token = this.tokens[this.pos + 1]
    if (!token) return false
    if (token.type === 'number') return true
    if (token.type === 'paren' && token.value === '(') return true
    if (token.type === 'name' && token.value.toLowerCase() !== 'of') return true
    return false
  }

  private unary(): Val {
    if (this.currentOpMatches('+', '-')) {
      const op = this.eat().value
      const value = this.unary()
      return op === '+' ? value : { v: -value.v, pct: value.pct }
    }
    return this.primary()
  }

  private primary(): Val {
    const token = this.peek()

    if (token.type === 'number') {
      this.pos++
      return { v: token.value, pct: false }
    }

    if (token.type === 'name') {
      this.pos++
      const name = token.value
      if (name.toLowerCase() === 'pi') return { v: Math.PI, pct: false }
      if (name.toLowerCase() === 'e') return { v: Math.E, pct: false }

      if (this.currentParenMatches('(')) {
        this.eat()
        const arg = this.expr()
        if (!this.currentParenMatches(')')) throw new Error('Expected closing parenthesis')
        this.eat()
        return { v: applyFunction(name, this.resolve(arg)), pct: false }
      }
      throw new Error(`Unknown identifier: ${name}`)
    }

    if (this.currentParenMatches('(')) {
      this.eat()
      const value = this.expr()
      if (!this.currentParenMatches(')')) throw new Error('Expected closing parenthesis')
      this.eat()
      return value
    }

    throw new Error(`Unexpected token: ${token.type}`)
  }

  private currentOpMatches(...ops: string[]): boolean {
    const token = this.current()
    return token.type === 'op' && ops.includes(token.value)
  }

  private currentNameMatches(name: string): boolean {
    const token = this.current()
    return token.type === 'name' && token.value.toLowerCase() === name
  }

  private currentParenMatches(value: '(' | ')'): boolean {
    const token = this.current()
    return token.type === 'paren' && token.value === value
  }
}

function applyFunction(name: string, arg: number): number {
  switch (name.toLowerCase()) {
    case 'abs':
      return Math.abs(arg)
    case 'acos':
      return Math.acos(arg)
    case 'asin':
      return Math.asin(arg)
    case 'atan':
      return Math.atan(arg)
    case 'ceil':
      return Math.ceil(arg)
    case 'cos':
      return Math.cos(arg)
    case 'exp':
      return Math.exp(arg)
    case 'floor':
      return Math.floor(arg)
    case 'log':
      return Math.log(arg)
    case 'log10':
      return Math.log10(arg)
    case 'log2':
      return Math.log2(arg)
    case 'round':
      return Math.round(arg)
    case 'sign':
      return Math.sign(arg)
    case 'sin':
      return Math.sin(arg)
    case 'sqrt':
      return Math.sqrt(arg)
    case 'tan':
      return Math.tan(arg)
    case 'trunc':
      return Math.trunc(arg)
    default:
      throw new Error(`Unknown function: ${name}`)
  }
}

function evaluateMath(input: string): number | null {
  if (!input.trim()) return null
  try {
    const result = new Parser(input).parse()
    if (!Number.isFinite(result)) return null
    return result
  } catch {
    return null
  }
}

// ── Smart evaluation (units, dates, base display) ─────────────────────────────

export interface CalcResult {
  // Primary display, also what gets copied
  label: string
  sublabel?: string
}

export interface CurrencyRates {
  base: string
  date: string
  // Currency code (uppercase) -> units per 1 base unit; includes the base at 1.
  rates: Record<string, number>
}

function formatNumber(value: number): string {
  const rounded = Math.round(value * 1_000_000) / 1_000_000
  return String(rounded)
}

// Conversion factors to a per-group base unit (m, kg, byte, second)
const UNITS: Record<string, { group: string; factor: number }> = {
  // length (base: meter)
  mm: { group: 'length', factor: 0.001 },
  cm: { group: 'length', factor: 0.01 },
  m: { group: 'length', factor: 1 },
  km: { group: 'length', factor: 1000 },
  in: { group: 'length', factor: 0.0254 },
  inch: { group: 'length', factor: 0.0254 },
  ft: { group: 'length', factor: 0.3048 },
  feet: { group: 'length', factor: 0.3048 },
  foot: { group: 'length', factor: 0.3048 },
  yd: { group: 'length', factor: 0.9144 },
  yard: { group: 'length', factor: 0.9144 },
  mi: { group: 'length', factor: 1609.344 },
  mile: { group: 'length', factor: 1609.344 },
  // mass (base: kilogram)
  mg: { group: 'mass', factor: 1e-6 },
  g: { group: 'mass', factor: 0.001 },
  kg: { group: 'mass', factor: 1 },
  lb: { group: 'mass', factor: 0.453_592_37 },
  lbs: { group: 'mass', factor: 0.453_592_37 },
  pound: { group: 'mass', factor: 0.453_592_37 },
  oz: { group: 'mass', factor: 0.028_349_523_125 },
  ton: { group: 'mass', factor: 1000 },
  tonne: { group: 'mass', factor: 1000 },
  // data (base: byte, decimal + binary prefixes)
  b: { group: 'data', factor: 1 },
  byte: { group: 'data', factor: 1 },
  kb: { group: 'data', factor: 1e3 },
  mb: { group: 'data', factor: 1e6 },
  gb: { group: 'data', factor: 1e9 },
  tb: { group: 'data', factor: 1e12 },
  kib: { group: 'data', factor: 1024 },
  mib: { group: 'data', factor: 1024 ** 2 },
  gib: { group: 'data', factor: 1024 ** 3 },
  tib: { group: 'data', factor: 1024 ** 4 },
  // time (base: second)
  ms: { group: 'time', factor: 0.001 },
  s: { group: 'time', factor: 1 },
  sec: { group: 'time', factor: 1 },
  second: { group: 'time', factor: 1 },
  min: { group: 'time', factor: 60 },
  minute: { group: 'time', factor: 60 },
  h: { group: 'time', factor: 3600 },
  hr: { group: 'time', factor: 3600 },
  hour: { group: 'time', factor: 3600 },
  day: { group: 'time', factor: 86_400 },
  week: { group: 'time', factor: 604_800 },
  year: { group: 'time', factor: 31_536_000 },
}

const TEMP_UNITS: Record<string, string> = {
  c: 'c',
  celsius: 'c',
  f: 'f',
  fahrenheit: 'f',
  k: 'k',
  kelvin: 'k',
}

function lookupUnit(raw: string): string | null {
  const u = raw.toLowerCase()
  if (UNITS[u] || TEMP_UNITS[u]) return u
  // plural fallback: "miles" → "mile"
  const singular = u.replace(/s$/, '')
  if (UNITS[singular] || TEMP_UNITS[singular]) return singular
  return null
}

function convertTemp(value: number, from: string, to: string): number {
  const kelvin = from === 'c' ? value + 273.15 : from === 'f' ? ((value - 32) * 5) / 9 + 273.15 : value
  return to === 'c' ? kelvin - 273.15 : to === 'f' ? ((kelvin - 273.15) * 9) / 5 + 32 : kelvin
}

function tryUnits(input: string): CalcResult | null {
  const m = /^(.+?)\s*([a-zA-Z]+)\s+(?:in|to|as)\s+([a-zA-Z]+)$/i.exec(input)
  if (!m) return null
  const fromUnit = lookupUnit(m[2])
  const toUnit = lookupUnit(m[3])
  if (!fromUnit || !toUnit) return null
  const value = evaluateMath(m[1])
  if (value === null) return null

  let out: number
  if (TEMP_UNITS[fromUnit] && TEMP_UNITS[toUnit]) {
    out = convertTemp(value, TEMP_UNITS[fromUnit], TEMP_UNITS[toUnit])
  } else {
    const from = UNITS[fromUnit]
    const to = UNITS[toUnit]
    if (!from || !to || from.group !== to.group) return null
    out = (value * from.factor) / to.factor
  }
  if (!Number.isFinite(out)) return null
  return {
    label: `${formatNumber(out)} ${m[3]}`,
    sublabel: `${formatNumber(value)} ${m[2]} = ${formatNumber(out)} ${m[3]}`,
  }
}

// Currency symbols the calculator understands on either side of a conversion.
const CURRENCY_SYMBOLS: Record<string, string> = {
  $: 'USD',
  '€': 'EUR',
  '£': 'GBP',
  '¥': 'JPY',
}

// Resolve a symbol ("$") or ISO code ("usd") to an uppercase code that is
// actually present in the rates table; null if it isn't a known currency.
function currencyCode(token: string, rates: CurrencyRates): string | null {
  const code = (CURRENCY_SYMBOLS[token] ?? token).toUpperCase()
  return rates.rates[code] !== undefined ? code : null
}

function formatMoney(value: number): string {
  // Two decimals for typical amounts; more precision for sub-cent values.
  const abs = Math.abs(value)
  const digits = abs !== 0 && abs < 0.01 ? 6 : 2
  return value.toLocaleString('en-US', {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  })
}

// "100 usd in eur", "$100 to gbp", "10 * 2 eur as usd". Only fires when both
// sides resolve to known currency codes, so unit/date/arithmetic inputs fall
// through untouched.
function tryCurrency(input: string, rates: CurrencyRates): CalcResult | null {
  const m = /^(.+?)\s+(?:in|to|as)\s+(.+?)$/i.exec(input.trim())
  if (!m) return null
  const toCur = currencyCode(m[2].trim(), rates)
  if (!toCur) return null

  // The left side is "<amount><currency>": a leading symbol ("$100") or a
  // trailing code ("100 usd" / "10*2 usd").
  let amountStr = m[1].trim()
  let fromCur: string | null = null

  const lead = /^([$€£¥])\s*(.+)$/.exec(amountStr)
  const trail = /^(.*?)\s*([a-z]{3})$/i.exec(amountStr)
  if (lead) {
    fromCur = currencyCode(lead[1], rates)
    if (fromCur) amountStr = lead[2].trim()
  } else if (trail && currencyCode(trail[2], rates)) {
    fromCur = currencyCode(trail[2], rates)
    amountStr = trail[1].trim()
  }
  if (!fromCur) return null

  const amount = evaluateMath(amountStr)
  if (amount === null) return null

  const out = (amount * rates.rates[toCur]) / rates.rates[fromCur]
  if (!Number.isFinite(out)) return null
  const asOf = rates.date ? ` · rates ${rates.date}` : ''
  return {
    label: `${formatMoney(out)} ${toCur}`,
    sublabel: `${formatMoney(amount)} ${fromCur} = ${formatMoney(out)} ${toCur}${asOf}`,
  }
}

function startOfToday(): Date {
  const d = new Date()
  d.setHours(0, 0, 0, 0)
  return d
}

function parseLooseDate(s: string): Date | null {
  const now = new Date()
  // "dec 25" — assume this year, roll to next year if already past
  const withYear = new Date(`${s} ${now.getFullYear()}`)
  if (!Number.isNaN(withYear.getTime())) {
    if (withYear < startOfToday()) withYear.setFullYear(now.getFullYear() + 1)
    return withYear
  }
  const direct = new Date(s)
  return Number.isNaN(direct.getTime()) ? null : direct
}

const DATE_UNIT_MS: Record<string, number> = {
  minute: 60_000,
  min: 60_000,
  hour: 3_600_000,
  hr: 3_600_000,
  h: 3_600_000,
  day: 86_400_000,
  d: 86_400_000,
  week: 604_800_000,
  w: 604_800_000,
}

function tryDates(input: string): CalcResult | null {
  let m = /^days?\s+until\s+(.+)$/i.exec(input)
  if (m) {
    const target = parseLooseDate(m[1])
    if (!target) return null
    const days = Math.round((target.getTime() - startOfToday().getTime()) / 86_400_000)
    return {
      label: `${days} day${days === 1 ? '' : 's'}`,
      sublabel: target.toDateString(),
    }
  }

  m =
    /^(now|today|tomorrow)\s*([+-])\s*(\d+(?:\.\d+)?)\s*(minutes?|mins?|hours?|hrs?|days?|weeks?|months?|years?)$/i.exec(
      input,
    )
  if (m) {
    const base = new Date()
    if (m[1].toLowerCase() === 'tomorrow') base.setDate(base.getDate() + 1)
    const sign = m[2] === '-' ? -1 : 1
    const amount = parseFloat(m[3]) * sign
    const unit = m[4].toLowerCase().replace(/s$/, '')
    const result = new Date(base)
    if (unit === 'month') result.setMonth(result.getMonth() + amount)
    else if (unit === 'year') result.setFullYear(result.getFullYear() + amount)
    else {
      const ms = DATE_UNIT_MS[unit]
      if (!ms) return null
      result.setTime(result.getTime() + amount * ms)
    }
    const dayGranularity = !['minute', 'min', 'hour', 'hr'].includes(unit)
    return {
      label: dayGranularity ? result.toDateString() : result.toLocaleString(),
      sublabel: `${m[1]} ${m[2]} ${m[3]} ${m[4]}`,
    }
  }

  return null
}

function tryArithmetic(input: string): CalcResult | null {
  let parser: Parser
  try {
    parser = new Parser(input)
  } catch {
    return null
  }
  let value: number
  try {
    value = parser.parse()
  } catch {
    return null
  }
  if (!Number.isFinite(value)) return null

  let sublabel: string | undefined
  if (parser.sawBaseLiteral && Number.isInteger(value) && value >= 0 && value < 2 ** 53) {
    sublabel = `hex 0x${value.toString(16)} · bin 0b${value.toString(2)}`
  }
  return { label: formatNumber(value), sublabel }
}

// Calculator entry point: arithmetic (with %, hex/bin literals), unit
// conversions ("5 km in mi"), currency ("100 usd in eur", when `rates` is
// supplied), and date math ("days until dec 25").
export function evaluateSmart(input: string, rates?: CurrencyRates): CalcResult | null {
  const trimmed = input.trim()
  if (!trimmed) return null
  const currency = rates ? tryCurrency(trimmed, rates) : null
  return tryDates(trimmed) ?? currency ?? tryUnits(trimmed) ?? tryArithmetic(trimmed)
}
