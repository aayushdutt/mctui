// Checks a frame against the rules terminals with line spacing impose
// (DESIGN.md "Every row is one line"): what would split into stripes or
// dashes on the user's terminal.

import type { Buffer } from './term/buffer'

const VERTICAL = new Set([...'│┃║┆┇┊┋╎╏├┤┼┣┫╋╠╣╬┌┐└┘┏┓┗┛╔╗╚╝╭╮╯╰┬┴┳┻╦╩╽╿'])
const BLOCK = /[\u2580-\u259f]/

export interface Finding {
  rule: string
  detail: string
  count: number
}

export function lint(buf: Buffer): Finding[] {
  let strokes = 0
  let stacked = 0
  let blocks = 0
  const fgs = new Set<string>()
  for (let y = 0; y < buf.rows; y++)
    for (let x = 0; x < buf.cols; x++) {
      const c = buf.cells[y * buf.cols + x]
      if (c.ch.trim() && c.st.fg) fgs.add(c.st.fg)
      if (!VERTICAL.has(c.ch)) {
        // fall through to the tint check
      } else {
        const below = buf.get(x, y + 1)
        const above = buf.get(x, y - 1)
        if ((below && VERTICAL.has(below.ch)) || (above && VERTICAL.has(above.ch))) strokes++
      }
      const below = buf.get(x, y + 1)
      if (c.st.bg && below?.st.bg === c.st.bg) stacked++
      if (BLOCK.test(c.ch) && below && BLOCK.test(below.ch)) blocks++
    }
  const out: Finding[] = []
  if (strokes)
    out.push({ rule: 'vertical strokes', detail: 'glyphs meant to join across rows; line spacing breaks them into dashes', count: strokes })
  if (stacked)
    out.push({ rule: 'multi-row tint', detail: 'the same background on stacked rows; line spacing stripes it', count: stacked })
  if (blocks)
    out.push({ rule: 'stacked blocks', detail: 'block glyphs on consecutive rows (big type, sprites); line spacing leaves a hairline between them', count: blocks })
  out.push({ rule: 'colours', detail: 'distinct foreground colours on screen', count: fgs.size })
  return out
}
