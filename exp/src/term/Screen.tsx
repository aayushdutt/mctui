// Renders a Buffer as DOM, the way a terminal would: a fixed grid of cells.
//
// `gap` mimics terminals with line spacing (the user's does this): each row
// is taller than its glyphs, backgrounds only cover the glyph height, and
// box-drawing strokes don't bridge the gap. Designs that only look right
// without gaps show their seams here.

import { memo, useMemo, type CSSProperties, type ReactNode } from 'react'
import { type Buffer, type Cell, type Style } from './buffer'
import { mix } from './theme'
import { boxGlyph } from './glyphs'

export interface ScreenOptions {
  /** Font size in px. */
  size: number
  /** Row height as a multiple of the font size (1.2 ≈ no spacing). */
  lineHeight: number
  /** Simulate a terminal that leaves the line spacing unpainted. */
  gap: boolean
  font: string
  /** Terminal default foreground/background. */
  fg: string
  bg: string
}

/** The height glyphs (and, in gap mode, backgrounds) occupy, in em. */
const GLYPH_EM = 1.25

interface Run {
  st: Style
  text: string
  width: number
  x: number
}

function sameStyle(a: Style, b: Style) {
  return (
    a.fg === b.fg &&
    a.bg === b.bg &&
    !!a.bold === !!b.bold &&
    !!a.dim === !!b.dim &&
    !!a.italic === !!b.italic &&
    !!a.underline === !!b.underline &&
    !!a.reverse === !!b.reverse &&
    !!a.strike === !!b.strike
  )
}

const isAscii = (ch: string) => ch.length === 1 && ch.charCodeAt(0) < 0x80

/** Row cells grouped into runs of one style, splitting out special glyphs. */
function runs(cells: Cell[]): Run[] {
  const out: Run[] = []
  cells.forEach((c, x) => {
    if (c.cont) return
    const w = cells[x + 1]?.cont ? 2 : 1
    const last = out[out.length - 1]
    // ASCII runs merge; anything else gets its own fixed-width cell so a
    // fallback font can't knock the grid out of line.
    if (last && isAscii(c.ch) && isAscii(last.text.slice(-1)) && sameStyle(last.st, c.st)) {
      last.text += c.ch
      last.width += w
    } else out.push({ st: c.st, text: c.ch, width: w, x })
  })
  return out
}

function colors(st: Style, o: ScreenOptions) {
  let fg = st.fg ?? o.fg
  let bg = st.bg
  if (st.reverse) [fg, bg] = [bg ?? o.bg, fg]
  if (st.dim) fg = mix(fg, bg ?? o.bg, 0.4)
  return { fg, bg }
}

const Row = memo(function Row({ cells, o, cursorX }: { cells: Cell[]; o: ScreenOptions; cursorX: number | null }) {
  const children: ReactNode[] = []
  for (const run of runs(cells)) {
    const { fg, bg } = colors(run.st, o)
    const style: CSSProperties = {
      width: `${run.width}ch`,
      color: fg,
      fontWeight: run.st.bold ? 700 : 400,
      fontStyle: run.st.italic ? 'italic' : undefined,
      textDecoration:
        [run.st.underline && 'underline', run.st.strike && 'line-through'].filter(Boolean).join(' ') || undefined,
    }
    if (bg) {
      style.background = o.gap
        ? `linear-gradient(${bg}, ${bg}) center / 100% ${Math.min(GLYPH_EM, o.lineHeight)}em no-repeat`
        : bg
    }
    const glyph = run.text.length <= 2 ? boxGlyph(run.text, fg, o.gap ? Math.min(1, GLYPH_EM / o.lineHeight) : 1) : null
    children.push(
      <span key={run.x} className="term-run" style={style}>
        {glyph ?? run.text}
      </span>,
    )
  }
  if (cursorX !== null) {
    children.push(<span key="cursor" className="term-cursor" style={{ left: `${cursorX}ch`, background: o.fg }} />)
  }
  return <div className="term-row">{children}</div>
})

export function Screen({ buf, o, className }: { buf: Buffer; o: ScreenOptions; className?: string }) {
  const rowsCells = useMemo(
    () => Array.from({ length: buf.rows }, (_, y) => buf.cells.slice(y * buf.cols, (y + 1) * buf.cols)),
    [buf],
  )
  return (
    <div
      aria-hidden
      className={`term ${className ?? ''}`}
      style={{
        fontFamily: o.font,
        fontSize: `${o.size}px`,
        lineHeight: `${o.lineHeight}em`,
        width: `${buf.cols}ch`,
        color: o.fg,
        background: o.bg,
      }}
    >
      {rowsCells.map((cells, y) => (
        <Row key={y} cells={cells} o={o} cursorX={buf.cursor?.y === y ? buf.cursor.x : null} />
      ))}
    </div>
  )
}
