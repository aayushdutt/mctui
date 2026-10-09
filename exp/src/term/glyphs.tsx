// Box-drawing and block glyphs drawn as SVG, the way terminals draw them
// themselves rather than trusting the font: strokes meet exactly at cell
// edges, so boxes and bars join up. `cover` is how much of the row's height
// the glyph may use (1 = the whole row; less leaves the line-spacing gap
// unpainted, as the user's terminal does).

import type { ReactNode } from 'react'

// Segment weights: 0 none, 1 light, 2 heavy, 3 double. Order: up right down left.
type Arms = [number, number, number, number]

const ARMS: Record<string, Arms> = {
  '─': [0, 1, 0, 1], '│': [1, 0, 1, 0], '┌': [0, 1, 1, 0], '┐': [0, 0, 1, 1], '└': [1, 1, 0, 0], '┘': [1, 0, 0, 1],
  '├': [1, 1, 1, 0], '┤': [1, 0, 1, 1], '┬': [0, 1, 1, 1], '┴': [1, 1, 0, 1], '┼': [1, 1, 1, 1],
  '━': [0, 2, 0, 2], '┃': [2, 0, 2, 0], '┏': [0, 2, 2, 0], '┓': [0, 0, 2, 2], '┗': [2, 2, 0, 0], '┛': [2, 0, 0, 2],
  '┣': [2, 2, 2, 0], '┫': [2, 0, 2, 2], '┳': [0, 2, 2, 2], '┻': [2, 2, 0, 2], '╋': [2, 2, 2, 2],
  '┝': [1, 2, 1, 0], '┥': [1, 0, 1, 2], '┠': [2, 1, 2, 0], '┨': [2, 0, 2, 1], '┯': [0, 2, 1, 2], '┷': [1, 2, 0, 2],
  '═': [0, 3, 0, 3], '║': [3, 0, 3, 0], '╔': [0, 3, 3, 0], '╗': [0, 0, 3, 3], '╚': [3, 3, 0, 0], '╝': [3, 0, 0, 3],
  '╠': [3, 3, 3, 0], '╣': [3, 0, 3, 3], '╦': [0, 3, 3, 3], '╩': [3, 3, 0, 3], '╬': [3, 3, 3, 3],
  '╞': [1, 3, 1, 0], '╡': [1, 0, 1, 3], '╟': [3, 1, 3, 0], '╢': [3, 0, 3, 1], '╤': [0, 3, 1, 3], '╧': [1, 3, 0, 3],
  '╴': [0, 0, 0, 1], '╵': [1, 0, 0, 0], '╶': [0, 1, 0, 0], '╷': [0, 0, 1, 0],
  '╸': [0, 0, 0, 2], '╹': [2, 0, 0, 0], '╺': [0, 2, 0, 0], '╻': [0, 0, 2, 0],
  '╼': [0, 2, 0, 1], '╾': [0, 1, 0, 2], '╽': [1, 0, 2, 0], '╿': [2, 0, 1, 0],
}

const ROUND: Record<string, [number, number, number, number]> = {
  // from (x, y) curving through the centre to (x, y), in 0..100 units
  '╭': [100, 50, 50, 100],
  '╮': [0, 50, 50, 100],
  '╯': [0, 50, 50, 0],
  '╰': [100, 50, 50, 0],
}

const DASHED: Record<string, [horizontal: boolean, heavy: boolean, dashes: number]> = {
  '┄': [true, false, 3], '┅': [true, true, 3], '┈': [true, false, 4], '┉': [true, true, 4], '╌': [true, false, 2], '╍': [true, true, 2],
  '┆': [false, false, 3], '┇': [false, true, 3], '┊': [false, false, 4], '┋': [false, true, 4], '╎': [false, false, 2], '╏': [false, true, 2],
}

// Block elements as rectangles [x0, y0, x1, y1] in eighths of the cell.
const BLOCKS: Record<string, [number, number, number, number][]> = {
  '█': [[0, 0, 8, 8]], '▀': [[0, 0, 8, 4]], '▄': [[0, 4, 8, 8]], '▌': [[0, 0, 4, 8]], '▐': [[4, 0, 8, 8]],
  '▁': [[0, 7, 8, 8]], '▂': [[0, 6, 8, 8]], '▃': [[0, 5, 8, 8]], '▅': [[0, 3, 8, 8]], '▆': [[0, 2, 8, 8]], '▇': [[0, 1, 8, 8]],
  '▔': [[0, 0, 8, 1]], '▉': [[0, 0, 7, 8]], '▊': [[0, 0, 6, 8]], '▋': [[0, 0, 5, 8]], '▍': [[0, 0, 3, 8]], '▎': [[0, 0, 2, 8]], '▏': [[0, 0, 1, 8]],
  '▕': [[7, 0, 8, 8]],
  '▖': [[0, 4, 4, 8]], '▗': [[4, 4, 8, 8]], '▘': [[0, 0, 4, 4]], '▝': [[4, 0, 8, 4]],
  '▙': [[0, 0, 4, 8], [4, 4, 8, 8]], '▚': [[0, 0, 4, 4], [4, 4, 8, 8]], '▛': [[0, 0, 8, 4], [0, 4, 4, 8]],
  '▜': [[0, 0, 8, 4], [4, 4, 8, 8]], '▞': [[4, 0, 8, 4], [0, 4, 4, 8]], '▟': [[4, 0, 8, 8], [0, 4, 4, 8]],
}

const SHADES: Record<string, number> = { '░': 0.25, '▒': 0.5, '▓': 0.75 }

const px = { light: 1, heavy: 2.4 }

export function boxGlyph(ch: string, color: string, cover: number): ReactNode | null {
  const top = 50 - 50 * cover
  const bot = 50 + 50 * cover
  const svg = (children: ReactNode) => (
    <svg className="term-glyph" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden>
      {children}
    </svg>
  )
  const line = (x1: number, y1: number, x2: number, y2: number, w: number, key: string | number) => (
    <line
      key={key}
      x1={x1}
      y1={y1}
      x2={x2}
      y2={y2}
      stroke={color}
      strokeWidth={w}
      vectorEffect="non-scaling-stroke"
      shapeRendering="crispEdges"
    />
  )

  const arms = ARMS[ch]
  if (arms) {
    const [u, r, d, l] = arms
    const els: ReactNode[] = []
    // A double line is two light strokes either side of the centre line.
    const seg = (w: number, x1: number, y1: number, x2: number, y2: number, vertical: boolean, key: string) => {
      if (!w) return
      if (w === 3) {
        const off = vertical ? 18 : 10
        if (vertical) {
          els.push(line(x1 - off, y1, x2 - off, y2, px.light, key + 'a'), line(x1 + off, y1, x2 + off, y2, px.light, key + 'b'))
        } else {
          els.push(line(x1, y1 - off, x2, y2 - off, px.light, key + 'a'), line(x1, y1 + off, x2, y2 + off, px.light, key + 'b'))
        }
      } else els.push(line(x1, y1, x2, y2, w === 2 ? px.heavy : px.light, key))
    }
    seg(u, 50, top, 50, 50, true, 'u')
    seg(d, 50, 50, 50, bot, true, 'd')
    seg(l, 0, 50, 50, 50, false, 'l')
    seg(r, 50, 50, 100, 50, false, 'r')
    return svg(els)
  }

  const round = ROUND[ch]
  if (round) {
    const [x1, y1, x2, y2] = round
    const yEnd = y2 === 100 ? bot : top
    return svg(
      <path
        d={`M ${x1} ${y1} Q 50 50 50 ${y1 + (yEnd - y1) * 0.5} L ${x2} ${yEnd}`}
        fill="none"
        stroke={color}
        strokeWidth={px.light}
        vectorEffect="non-scaling-stroke"
      />,
    )
  }

  const dashed = DASHED[ch]
  if (dashed) {
    const [horizontal, heavy, n] = dashed
    const els: ReactNode[] = []
    const w = heavy ? px.heavy : px.light
    for (let i = 0; i < n; i++) {
      if (horizontal) {
        const step = 100 / n
        els.push(line(i * step + step * 0.2, 50, (i + 1) * step - step * 0.2, 50, w, i))
      } else {
        const step = (bot - top) / n
        els.push(line(50, top + i * step + step * 0.2, 50, top + (i + 1) * step - step * 0.2, w, i))
      }
    }
    return svg(els)
  }

  const y = (e: number) => top + ((bot - top) * e) / 8
  const blocks = BLOCKS[ch]
  if (blocks) {
    return svg(
      blocks.map(([x0, y0, x1, y1], i) => (
        <rect key={i} x={(x0 * 100) / 8} y={y(y0)} width={((x1 - x0) * 100) / 8} height={y(y1) - y(y0)} fill={color} shapeRendering="crispEdges" />
      )),
    )
  }

  const shade = SHADES[ch]
  if (shade) return svg(<rect x={0} y={top} width={100} height={bot - top} fill={color} opacity={shade} />)

  return null
}
