// Display type for the Launcher direction: capitals drawn with half blocks in
// the foreground colour only (no background ever spans rows), so terminals with
// line spacing show a thin gap inside each letter and nothing else breaks.
//
//   XL 6 rows: L with every pixel a whole cell, two wide. Big terminals only.
//   L  3 rows, 6 px tall, mostly 4 px wide: the stage title.
//   S  2 rows, 4 px tall, mostly 3 px wide: headers, and the stage when L won't fit.
//   T  1 row, letterspaced bold capitals: the last resort (and the card titles).

import type { Buffer, Color, Style } from '../../term/buffer'

export interface Font {
  id: 'XL' | 'L' | 'S'
  rows: number
  /** Cells between glyphs. */
  gap: number
  glyphs: Record<string, string[]>
}

/** Pixel art (`#` ink, `.` paper), two pixel rows per cell row. */
const px = (...rows: string[]): string[] => {
  const out: string[] = []
  for (let r = 0; r < rows.length; r += 2) {
    const top = rows[r]
    const bot = rows[r + 1] ?? ''
    let line = ''
    for (let x = 0; x < top.length; x++) {
      const a = top[x] === '#'
      const b = bot[x] === '#'
      line += a && b ? '█' : a ? '▀' : b ? '▄' : ' '
    }
    out.push(line)
  }
  return out
}

// prettier-ignore
const L: Record<string, string[]> = {
  A: px('.##.', '#..#', '#..#', '####', '#..#', '#..#'),
  B: px('###.', '#..#', '###.', '#..#', '#..#', '###.'),
  C: px('.###', '#...', '#...', '#...', '#...', '.###'),
  D: px('###.', '#..#', '#..#', '#..#', '#..#', '###.'),
  E: px('####', '#...', '###.', '#...', '#...', '####'),
  F: px('####', '#...', '###.', '#...', '#...', '#...'),
  G: px('.###', '#...', '#...', '#.##', '#..#', '.###'),
  H: px('#..#', '#..#', '####', '#..#', '#..#', '#..#'),
  I: px('###', '.#.', '.#.', '.#.', '.#.', '###'),
  J: px('...#', '...#', '...#', '...#', '#..#', '.##.'),
  K: px('#..#', '#.#.', '##..', '#.#.', '#..#', '#..#'),
  L: px('#...', '#...', '#...', '#...', '#...', '####'),
  M: px('#...#', '##.##', '#.#.#', '#...#', '#...#', '#...#'),
  N: px('#..#', '##.#', '#.##', '#..#', '#..#', '#..#'),
  O: px('.##.', '#..#', '#..#', '#..#', '#..#', '.##.'),
  P: px('###.', '#..#', '#..#', '###.', '#...', '#...'),
  Q: px('.##.', '#..#', '#..#', '#..#', '#.#.', '.#.#'),
  R: px('###.', '#..#', '#..#', '###.', '#.#.', '#..#'),
  S: px('.###', '#...', '.##.', '...#', '...#', '###.'),
  T: px('#####', '..#..', '..#..', '..#..', '..#..', '..#..'),
  U: px('#..#', '#..#', '#..#', '#..#', '#..#', '.##.'),
  V: px('#...#', '#...#', '#...#', '.#.#.', '.#.#.', '..#..'),
  W: px('#...#', '#...#', '#...#', '#.#.#', '##.##', '#...#'),
  X: px('#...#', '.#.#.', '..#..', '..#..', '.#.#.', '#...#'),
  Y: px('#...#', '.#.#.', '..#..', '..#..', '..#..', '..#..'),
  Z: px('####', '...#', '..#.', '.#..', '#...', '####'),
  '0': px('.##.', '#..#', '#.##', '##.#', '#..#', '.##.'),
  '1': px('.#.', '##.', '.#.', '.#.', '.#.', '###'),
  '2': px('.##.', '#..#', '...#', '.##.', '#...', '####'),
  '3': px('###.', '...#', '.##.', '...#', '...#', '###.'),
  '4': px('#..#', '#..#', '#..#', '####', '...#', '...#'),
  '5': px('####', '#...', '###.', '...#', '...#', '###.'),
  '6': px('.##.', '#...', '###.', '#..#', '#..#', '.##.'),
  '7': px('####', '...#', '..#.', '.#..', '.#..', '.#..'),
  '8': px('.##.', '#..#', '.##.', '#..#', '#..#', '.##.'),
  '9': px('.##.', '#..#', '#..#', '.###', '...#', '.##.'),
  '.': px('.', '.', '.', '.', '.', '#'),
  ',': px('.', '.', '.', '.', '#', '#'),
  '-': px('...', '...', '###', '...', '...', '...'),
  '+': px('...', '.#.', '###', '.#.', '...', '...'),
  "'": px('#', '#', '.', '.', '.', '.'),
  ':': px('.', '#', '.', '.', '#', '.'),
  '!': px('#', '#', '#', '#', '.', '#'),
  '?': px('###.', '...#', '.##.', '.#..', '....', '.#..'),
  '_': px('....', '....', '....', '....', '....', '####'),
  '/': px('..#', '..#', '.#.', '.#.', '#..', '#..'),
  '(': px('.#', '#.', '#.', '#.', '#.', '.#'),
  ')': px('#.', '.#', '.#', '.#', '.#', '#.'),
  ' ': ['  ', '  ', '  '],
}

// prettier-ignore
const S: Record<string, string[]> = {
  A: ['▄▀█', '█▀█'], B: ['█▄▄', '█▄█'], C: ['█▀▀', '█▄▄'], D: ['█▀▄', '█▄▀'],
  E: ['█▀▀', '██▄'], F: ['█▀▀', '█▀ '], G: ['█▀▀', '█▄█'], H: ['█ █', '█▀█'],
  I: ['█', '█'], J: ['  █', '█▄█'], K: ['█▄▀', '█ █'], L: ['█  ', '█▄▄'],
  M: ['█▀▄▀█', '█ ▀ █'], N: ['█▄ █', '█ ▀█'], O: ['█▀█', '█▄█'], P: ['█▀█', '█▀▀'],
  Q: ['█▀█', '▀▀█'], R: ['█▀█', '█▀▄'], S: ['█▀', '▄█'], T: ['▀█▀', ' █ '],
  U: ['█ █', '█▄█'], V: ['█ █', '▀▄▀'], W: ['█ █ █', '▀▄▀▄▀'], X: ['▀▄▀', '█ █'],
  Y: ['█▄█', ' █ '], Z: ['▀█', '█▄'],
  '0': ['█▀█', '█▄█'], '1': ['▄█', ' █'], '2': ['▀█', '█▄'], '3': ['▀▀█', '▄██'],
  '4': ['█ █', '▀▀█'], '5': ['█▀', '▄█'], '6': ['█▄▄', '█▄█'], '7': ['▀▀█', '  █'],
  '8': ['█▀█', '█▀█'], '9': ['█▀█', '▀▀█'],
  '.': [' ', '▄'], ',': [' ', '█'], '-': ['▄▄', '  '], '+': [' ▄ ', '▀█▀'], "'": ['▀', ' '],
  ':': ['▄', '▄'], '!': ['█', '▄'], '?': ['▀█', ' ▄'], '_': ['  ', '▄▄'], '/': [' █', '█ '],
  '(': ['▄▀', '▀▄'], ')': ['▀▄', '▄▀'], ' ': [' ', ' '],
}

export const FONT_L: Font = { id: 'L', rows: 3, gap: 1, glyphs: L }

/** L blown up: each half-block pixel becomes a full cell, two cells wide. */
const XL: Record<string, string[]> = Object.fromEntries(
  Object.entries(L).map(([ch, g]) => [
    ch,
    g.flatMap((line) => {
      const cells = [...line]
      const top = cells.map((c) => (c === '█' || c === '▀' ? '██' : '  ')).join('')
      const bot = cells.map((c) => (c === '█' || c === '▄' ? '██' : '  ')).join('')
      return [top, bot]
    }),
  ]),
)
export const FONT_XL: Font = { id: 'XL', rows: 6, gap: 2, glyphs: XL }
export const FONT_S: Font = { id: 'S', rows: 2, gap: 1, glyphs: S }

const glyph = (f: Font, ch: string) => f.glyphs[ch.toUpperCase()] ?? f.glyphs['?']

/** How many cells `text` takes set in `f`. */
export function measure(text: string, f: Font): number {
  const chars = [...text]
  if (!chars.length) return 0
  return chars.reduce((w, ch) => w + glyph(f, ch)[0].length, 0) + f.gap * (chars.length - 1)
}

/**
 * Sets `text` in `f` at (x, y). Only inked cells are written, so whatever is
 * behind shows through. `colors` gives each row its own shade (the last one
 * repeats). Returns the x after the last glyph.
 */
export function drawBig(buf: Buffer, x: number, y: number, text: string, f: Font, colors: Color[], max = buf.cols - x, bold = false): number {
  const limit = x + max
  for (const ch of text) {
    const g = glyph(f, ch)
    const gw = g[0].length
    if (x + gw > limit) break
    g.forEach((line, r) => {
      const st: Style = { fg: colors[Math.min(r, colors.length - 1)], bold }
      ;[...line].forEach((cell, i) => {
        if (cell !== ' ') buf.set(x + i, y + r, cell, st)
      })
    })
    x += gw + f.gap
  }
  return x - f.gap
}

/** `S U R V I V A L`: the one-row fallback. */
export const spaced = (text: string) =>
  [...text.toUpperCase()].map((ch) => (ch === ' ' ? ' ' : ch)).join(' ')

/** The biggest face that fits `text` in `w` cells, largest first. */
export function fit(text: string, w: number, faces: Font[] = [FONT_XL, FONT_L, FONT_S]): Font | null {
  return faces.find((f) => measure(text, f) <= w) ?? null
}
