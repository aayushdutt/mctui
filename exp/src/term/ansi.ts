// ANSI truecolor text <-> Buffer: reads the app's screenshots
// (`make screenshots` writes target/screenshots/*.ansi) and writes mockups
// in the same format, so scripts/ansi2png.py can render either.

import { Buffer, charWidth, type Color, type Style } from './buffer'

const ANSI16: Record<number, Color> = {
  30: '#000000', 31: '#cd3131', 32: '#0dbc79', 33: '#e5e510', 34: '#2472c8', 35: '#bc3fbc', 36: '#11a8cd', 37: '#e5e5e5',
  90: '#666666', 91: '#f14c4c', 92: '#23d18b', 93: '#f5f543', 94: '#3b8eea', 95: '#d670d6', 96: '#29b8db', 97: '#ffffff',
}

const hex = (r: number, g: number, b: number): Color =>
  '#' + [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('')

function xterm256(n: number): Color {
  if (n < 16) return Object.values(ANSI16)[n] ?? '#ffffff'
  if (n >= 232) {
    const v = 8 + (n - 232) * 10
    return hex(v, v, v)
  }
  const i = n - 16
  const step = (v: number) => (v === 0 ? 0 : 55 + v * 40)
  return hex(step(Math.floor(i / 36)), step(Math.floor(i / 6) % 6), step(i % 6))
}

function sgr(st: Style, params: number[]): Style {
  if (params.length === 0) params = [0]
  for (let k = 0; k < params.length; k++) {
    const p = params[k]
    if (p === 0) st = {}
    else if (p === 1) st = { ...st, bold: true }
    else if (p === 2) st = { ...st, dim: true }
    else if (p === 3) st = { ...st, italic: true }
    else if (p === 4) st = { ...st, underline: true }
    else if (p === 7) st = { ...st, reverse: true }
    else if (p === 9) st = { ...st, strike: true }
    else if (p === 22) st = { ...st, bold: false, dim: false }
    else if (p === 23) st = { ...st, italic: false }
    else if (p === 24) st = { ...st, underline: false }
    else if (p === 27) st = { ...st, reverse: false }
    else if (p === 29) st = { ...st, strike: false }
    else if (p === 39) st = { ...st, fg: undefined }
    else if (p === 49) st = { ...st, bg: undefined }
    else if (p === 38 || p === 48) {
      let c: Color | undefined
      if (params[k + 1] === 2) {
        c = hex(params[k + 2] ?? 0, params[k + 3] ?? 0, params[k + 4] ?? 0)
        k += 4
      } else if (params[k + 1] === 5) {
        c = xterm256(params[k + 2] ?? 0)
        k += 2
      }
      st = p === 38 ? { ...st, fg: c } : { ...st, bg: c }
    } else if (ANSI16[p]) st = { ...st, fg: ANSI16[p] }
    else if (p >= 40 && p <= 47) st = { ...st, bg: ANSI16[p - 10] }
    else if (p >= 100 && p <= 107) st = { ...st, bg: ANSI16[p - 10] }
  }
  return st
}

/** Parses ANSI text into a buffer sized to fit it. */
export function parseAnsi(text: string): Buffer {
  const lines = text.replace(/\n$/, '').split('\n')
  const parsed = lines.map((line) => {
    const cells: { ch: string; st: Style }[] = []
    let st: Style = {}
    for (const tok of line.split(/(\x1b\[[0-9;]*[A-Za-z])/)) {
      const m = /^\x1b\[([0-9;]*)([A-Za-z])$/.exec(tok)
      if (m) {
        if (m[2] === 'm') st = sgr(st, m[1] ? m[1].split(';').map((p) => parseInt(p || '0', 10)) : [])
        continue
      }
      for (const ch of tok) cells.push({ ch, st })
    }
    return cells
  })
  const cols = Math.max(1, ...parsed.map((r) => r.reduce((w, c) => w + charWidth(c.ch), 0)))
  const buf = new Buffer(cols, parsed.length)
  parsed.forEach((cells, y) => {
    let x = 0
    for (const c of cells) {
      buf.set(x, y, c.ch, c.st)
      x += charWidth(c.ch)
    }
  })
  return buf
}

const code = (st: Style): string => {
  const p = ['0']
  if (st.fg) p.push('38;2;' + rgb(st.fg))
  if (st.bg) p.push('48;2;' + rgb(st.bg))
  if (st.bold) p.push('1')
  if (st.dim) p.push('2')
  if (st.italic) p.push('3')
  if (st.underline) p.push('4')
  if (st.reverse) p.push('7')
  if (st.strike) p.push('9')
  return `\x1b[${p.join(';')}m`
}

const rgb = (c: Color) => {
  const n = parseInt(c.slice(1), 16)
  return `${(n >> 16) & 255};${(n >> 8) & 255};${n & 255}`
}

/** The buffer as ANSI truecolor text, one line per row. */
export function toAnsi(buf: Buffer): string {
  const out: string[] = []
  for (let y = 0; y < buf.rows; y++) {
    let line = ''
    let last = ''
    for (let x = 0; x < buf.cols; x++) {
      const c = buf.cells[y * buf.cols + x]
      if (c.cont) continue
      const s = code(c.st)
      if (s !== last) {
        line += s
        last = s
      }
      line += c.ch
    }
    out.push(line + '\x1b[0m')
  }
  return out.join('\n') + '\n'
}
