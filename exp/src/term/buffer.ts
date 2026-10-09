// A terminal frame: a grid of cells, drawn the way ratatui draws them.
//
// Mockups draw into a Buffer instead of HTML so that whatever we sketch here
// is something a real terminal can show: one glyph per cell, one fg/bg per
// cell, nothing between the rows.

export type Color = string // '#rrggbb'

export interface Style {
  fg?: Color
  bg?: Color
  bold?: boolean
  dim?: boolean
  italic?: boolean
  underline?: boolean
  reverse?: boolean
  strike?: boolean
}

export interface Cell {
  ch: string
  st: Style
  /** The right half of a wide glyph: drawn by the cell to its left. */
  cont?: boolean
}

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

/** A piece of styled text. A bare string takes the line's base style. */
export type Span = string | [string, Style]
export type Line = Span[]

export const rect = (x: number, y: number, w: number, h: number): Rect => ({
  x,
  y,
  w: Math.max(0, w),
  h: Math.max(0, h),
})

export const right = (r: Rect) => r.x + r.w
export const bottom = (r: Rect) => r.y + r.h

/** CSS-style inset: (all) | (vertical, horizontal) | (top, right, bottom, left). */
export function inset(r: Rect, t: number, rt = t, b = t, l = rt): Rect {
  return rect(r.x + l, r.y + t, r.w - l - rt, r.h - t - b)
}

/** The single row `dy` rows into `r`. */
export const row = (r: Rect, dy = 0): Rect => rect(r.x, r.y + dy, r.w, dy < r.h ? 1 : 0)

/** A size along one axis: cells, a percentage, or a share of what's left. */
export type Constraint = number | `${number}%` | `${number}fr`

function sizes(total: number, cs: Constraint[], gap: number): number[] {
  const free0 = total - gap * Math.max(0, cs.length - 1)
  const out = cs.map((c) => {
    if (typeof c === 'number') return c
    if (c.endsWith('%')) return Math.floor((free0 * parseFloat(c)) / 100)
    return 0
  })
  const fixed = out.reduce((a, b) => a + b, 0)
  const frs = cs.map((c) => (typeof c === 'string' && c.endsWith('fr') ? parseFloat(c) : 0))
  const frTotal = frs.reduce((a, b) => a + b, 0)
  let free = Math.max(0, free0 - fixed)
  if (frTotal > 0) {
    let used = 0
    frs.forEach((f, i) => {
      if (!f) return
      out[i] = Math.floor((free * f) / frTotal)
      used += out[i]
    })
    // Hand the rounding leftovers to the last fr.
    const last = frs.map((f, i) => (f ? i : -1)).filter((i) => i >= 0).pop()!
    out[last] += free - used
  }
  return out.map((n) => Math.max(0, n))
}

/** Splits `r` into side-by-side columns. */
export function cols(r: Rect, cs: Constraint[], gap = 0): Rect[] {
  let x = r.x
  return sizes(r.w, cs, gap).map((w) => {
    const out = rect(x, r.y, Math.min(w, right(r) - x), r.h)
    x += w + gap
    return out
  })
}

/** Splits `r` into stacked rows. */
export function rows(r: Rect, cs: Constraint[], gap = 0): Rect[] {
  let y = r.y
  return sizes(r.h, cs, gap).map((h) => {
    const out = rect(r.x, y, r.w, Math.min(h, bottom(r) - y))
    y += h + gap
    return out
  })
}

/** A `w`×`h` rect centred in `r`. */
export function centered(r: Rect, w: number, h: number): Rect {
  w = Math.min(w, r.w)
  h = Math.min(h, r.h)
  return rect(r.x + Math.floor((r.w - w) / 2), r.y + Math.floor((r.h - h) / 2), w, h)
}

/** How many cells a glyph takes: 2 for East Asian wide, 1 otherwise. */
export function charWidth(ch: string): number {
  const c = ch.codePointAt(0) ?? 0
  if (c < 0x1100) return 1
  if (
    (c >= 0x1100 && c <= 0x115f) ||
    (c >= 0x2e80 && c <= 0xa4cf && c !== 0x303f) ||
    (c >= 0xac00 && c <= 0xd7a3) ||
    (c >= 0xf900 && c <= 0xfaff) ||
    (c >= 0xfe30 && c <= 0xfe4f) ||
    (c >= 0xff00 && c <= 0xff60) ||
    (c >= 0xffe0 && c <= 0xffe6) ||
    (c >= 0x1f300 && c <= 0x1faff) ||
    (c >= 0x20000 && c <= 0x3fffd)
  )
    return 2
  return 1
}

export function width(s: string): number {
  let w = 0
  for (const ch of s) w += charWidth(ch)
  return w
}

/** `s` cut to `max` cells, ending in `…` when it had to cut. */
export function truncate(s: string, max: number): string {
  if (width(s) <= max) return s
  if (max <= 0) return ''
  let out = ''
  let w = 0
  for (const ch of s) {
    const cw = charWidth(ch)
    if (w + cw > max - 1) break
    out += ch
    w += cw
  }
  return out + '…'
}

/** Word-wraps `s` to lines of at most `max` cells. */
export function wrap(s: string, max: number): string[] {
  const out: string[] = []
  for (const para of s.split('\n')) {
    let line = ''
    for (const word of para.split(/\s+/).filter(Boolean)) {
      const next = line ? `${line} ${word}` : word
      if (width(next) <= max) line = next
      else {
        if (line) out.push(line)
        line = width(word) > max ? truncate(word, max) : word
      }
    }
    out.push(line)
  }
  return out
}

export const spanText = (s: Span) => (typeof s === 'string' ? s : s[0])
export const lineWidth = (l: Line) => l.reduce((w, s) => w + width(spanText(s)), 0)

/** Patches `over` onto `base`: fields `over` leaves undefined keep `base`'s. */
export function patch(base: Style, over: Style | undefined): Style {
  if (!over) return base
  const out: Style = { ...base }
  for (const k of Object.keys(over) as (keyof Style)[]) {
    if (over[k] !== undefined) (out as Record<string, unknown>)[k] = over[k]
  }
  return out
}

export interface Region {
  rect: Rect
  key: string
}

export type BorderKind = 'plain' | 'rounded' | 'thick' | 'double'

const BORDERS: Record<BorderKind, string> = {
  // top-left, top, top-right, right, bottom-right, bottom, bottom-left, left
  plain: '┌─┐│┘─└│',
  rounded: '╭─╮│╯─╰│',
  thick: '┏━┓┃┛━┗┃',
  double: '╔═╗║╝═╚║',
}

export class Buffer {
  readonly cells: Cell[]
  /** Clickable areas, last drawn wins (like the app's hit map). */
  readonly regions: Region[] = []
  /** Where the text cursor sits, if a field is focused. */
  cursor: { x: number; y: number } | null = null

  constructor(
    readonly cols: number,
    readonly rows: number,
  ) {
    this.cells = Array.from({ length: cols * rows }, () => ({ ch: ' ', st: {} }))
  }

  area(): Rect {
    return rect(0, 0, this.cols, this.rows)
  }

  get(x: number, y: number): Cell | undefined {
    if (x < 0 || y < 0 || x >= this.cols || y >= this.rows) return undefined
    return this.cells[y * this.cols + x]
  }

  /** Puts one glyph at (x, y), patching its style over the cell's. */
  set(x: number, y: number, ch: string, st?: Style) {
    const cell = this.get(x, y)
    if (!cell) return
    // Overwriting half of a wide glyph blanks the other half.
    if (cell.cont) {
      const left = this.get(x - 1, y)
      if (left) left.ch = ' '
    } else if (charWidth(cell.ch) === 2) {
      const next = this.get(x + 1, y)
      if (next) {
        next.ch = ' '
        next.cont = false
      }
    }
    cell.ch = ch
    cell.cont = false
    cell.st = patch(cell.st, st)
    if (charWidth(ch) === 2) {
      const next = this.get(x + 1, y)
      if (next) {
        next.ch = ''
        next.cont = true
        next.st = patch(next.st, st)
      }
    }
  }

  /** Patches `st` over every cell of `r` without touching the glyphs. */
  paint(r: Rect, st: Style) {
    for (let y = r.y; y < bottom(r); y++)
      for (let x = r.x; x < right(r); x++) {
        const cell = this.get(x, y)
        if (cell) cell.st = patch(cell.st, st)
      }
  }

  /** Fills `r` with `ch` in style `st` (a clean slate: old styles dropped). */
  fill(r: Rect, st: Style = {}, ch = ' ') {
    for (let y = r.y; y < bottom(r); y++)
      for (let x = r.x; x < right(r); x++) {
        const cell = this.get(x, y)
        if (!cell) continue
        cell.ch = ch
        cell.cont = false
        cell.st = { ...st }
      }
  }

  /** Blanks `r` back to the terminal's default. */
  clear(r: Rect) {
    this.fill(r)
  }

  /**
   * Writes `s` at (x, y), cut to `max` cells with an ellipsis. Returns the x
   * after the last glyph.
   */
  text(x: number, y: number, s: string, st?: Style, max = this.cols - x): number {
    s = truncate(s, max)
    for (const ch of s) {
      this.set(x, y, ch, st)
      x += charWidth(ch)
    }
    return x
  }

  /** Writes a line of spans, each patched over `base`. Returns the end x. */
  line(x: number, y: number, line: Line, base: Style = {}, max = this.cols - x): number {
    const start = x
    for (const span of line) {
      const [s, st] = typeof span === 'string' ? [span, undefined] : span
      const room = max - (x - start)
      if (room <= 0) break
      x = this.text(x, y, s, patch(base, st), room)
    }
    return x
  }

  /** A line flush right inside `r`'s first row. Returns its start x. */
  lineRight(r: Rect, line: Line, base: Style = {}): number {
    const w = Math.min(lineWidth(line), r.w)
    const x = right(r) - w
    this.line(x, r.y, line, base, w)
    return x
  }

  /** A line centred inside `r`'s first row. Returns its start x. */
  lineCenter(r: Rect, line: Line, base: Style = {}): number {
    const w = Math.min(lineWidth(line), r.w)
    const x = r.x + Math.floor((r.w - w) / 2)
    this.line(x, r.y, line, base, w)
    return x
  }

  /** A horizontal run of `ch`. */
  hline(x: number, y: number, w: number, st?: Style, ch = '─') {
    for (let i = 0; i < w; i++) this.set(x + i, y, ch, st)
  }

  /** A vertical run of `ch`. (Careful: terminals with line spacing split these.) */
  vline(x: number, y: number, h: number, st?: Style, ch = '│') {
    for (let i = 0; i < h; i++) this.set(x, y + i, ch, st)
  }

  /**
   * A bordered box. Titles sit in the top border (` Title `), footers in the
   * bottom one. Returns the inner area.
   */
  box(
    r: Rect,
    opts: {
      border?: BorderKind
      st?: Style
      title?: Line
      titleAlign?: 'left' | 'center' | 'right'
      footer?: Line
      fill?: Style
    } = {},
  ): Rect {
    if (r.w < 2 || r.h < 2) return rect(r.x, r.y, 0, 0)
    const g = BORDERS[opts.border ?? 'rounded']
    const st = opts.st
    if (opts.fill) this.fill(r, opts.fill)
    const [x0, y0, x1, y1] = [r.x, r.y, right(r) - 1, bottom(r) - 1]
    this.set(x0, y0, g[0], st)
    this.hline(x0 + 1, y0, r.w - 2, st, g[1])
    this.set(x1, y0, g[2], st)
    this.vline(x1, y0 + 1, r.h - 2, st, g[3])
    this.set(x1, y1, g[4], st)
    this.hline(x0 + 1, y1, r.w - 2, st, g[5])
    this.set(x0, y1, g[6], st)
    this.vline(x0, y0 + 1, r.h - 2, st, g[7])
    const label = (line: Line, y: number, align: 'left' | 'center' | 'right') => {
      const inner = rect(x0 + 2, y, r.w - 4, 1)
      const padded: Line = [' ', ...line, ' ']
      if (align === 'center') this.lineCenter(inner, padded, st)
      else if (align === 'right') this.lineRight(inner, padded, st)
      else this.line(inner.x, y, padded, st, inner.w)
    }
    if (opts.title) label(opts.title, y0, opts.titleAlign ?? 'left')
    if (opts.footer) label(opts.footer, y1, 'right')
    return inset(r, 1)
  }

  /** Marks `r` clickable as `key` (see the Ui key grammar in kit.ts). */
  region(r: Rect, key: string) {
    if (r.w > 0 && r.h > 0) this.regions.push({ rect: r, key })
  }

  /** The key of the topmost region at (x, y). */
  hit(x: number, y: number): string | null {
    for (let i = this.regions.length - 1; i >= 0; i--) {
      const { rect: r, key } = this.regions[i]
      if (x >= r.x && x < right(r) && y >= r.y && y < bottom(r)) return key
    }
    return null
  }

  /** Recolours every glyph `fg` with no background: the dimmed page behind a card. */
  fade(fg: Color) {
    for (const cell of this.cells) cell.st = { fg }
    this.regions.length = 0
  }

  /** The frame as plain text, one string per row. */
  toText(): string[] {
    const out: string[] = []
    for (let y = 0; y < this.rows; y++) {
      let s = ''
      for (let x = 0; x < this.cols; x++) {
        const c = this.cells[y * this.cols + x]
        if (!c.cont) s += c.ch
      }
      out.push(s)
    }
    return out
  }
}
