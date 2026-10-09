// Help is a page, not a floating card: `?` swaps the page for the keys and
// the sidebar stays put. Delete asks on one row at the foot of the page.

import type { Ctx } from '../../kit'
import type { Instance } from '../../data/mock'
import { rect, right, bottom } from '../../term/buffer'
import * as p from './parts'

const GROUPS: [string, [string, string][]][] = [
  [
    'instances',
    [
      ['1–9', 'jump straight to one'],
      ['↑↓ j k', 'move'],
      ['tab', 'sidebar ↔ page'],
      ['/', 'filter by name'],
      ['n', 'new instance'],
    ],
  ],
  [
    'this instance',
    [
      ['↵', 'play'],
      ['o', 'play offline'],
      ['m', 'mods'],
      ['e', 'edit its settings'],
      ['d', 'delete'],
    ],
  ],
  [
    'while it runs',
    [
      ['↵', 'the full log'],
      ['s', 'stop the game'],
      ['c', 'copy the log'],
    ],
  ],
  [
    'anywhere',
    [
      ['a', 'accounts'],
      [',', 'settings'],
      ['?', 'this page'],
      ['q', 'quit'],
    ],
  ],
]

export function helpPage(c: Ctx, L: p.Layout) {
  const { buf, t } = c
  const P = L.page
  p.title(c, P, ['Keys'], [['The same letter does the same thing on every page.', { fg: t.dim }]])
  const twoCols = P.w >= 60
  const colW = twoCols ? Math.floor(P.w / 2) : P.w
  const heights = GROUPS.map(([, ks]) => ks.length + 2)
  let y = [P.y + 3, P.y + 3]
  GROUPS.forEach(([name, keys], gi) => {
    const col = twoCols ? (gi % 2 === 0 ? 0 : 1) : 0
    const x = P.x + col * colW
    const at = twoCols ? y[col] : y[0]
    if (at + heights[gi] - 1 > bottom(P)) return
    p.label(c, x, at, name)
    keys.forEach(([k, d], i) => {
      buf.text(x, at + 1 + i, k, { fg: t.strong, bold: true })
      buf.text(x + 9, at + 1 + i, d, { fg: t.dim }, colW - 11)
    })
    if (twoCols) y[col] += heights[gi]
    else y[0] += heights[gi]
  })
  buf.region(P, 'close')
  p.footer(c, L, [['esc', 'back'], ['?', 'close']])
}

/** `d`: one row at the foot of the page asks, red, and says what goes. */
export function confirm(c: Ctx, L: p.Layout, inst: Instance) {
  const { buf, t, ui } = c
  if (ui.overlay !== 'confirm') return
  const P = L.page
  const y = bottom(P) - 1
  const row = p.pageRow(P, y)
  buf.fill(row, { bg: t.errorBg })
  const worlds = inst.worlds.length
  const what = worlds ? ` and ${worlds === 1 ? 'its world' : `its ${worlds} worlds`}` : ''
  const end = buf.line(P.x, y, [['Delete ', { fg: t.strong, bold: true }], [inst.name, { fg: t.strong, bold: true }], [`${what}?`, { fg: t.text }]], {}, P.w - 22)
  const keys = [
    ['y', 'delete', 'key:y'],
    ['esc', 'keep', 'close'],
  ] as const
  let x = Math.max(end + 3, right(P) - 20)
  for (const [k, l, region] of keys) {
    const start = x
    x = buf.text(x, y, k, { fg: t.error, bold: true })
    x = buf.text(x + 1, y, l, { fg: t.text })
    buf.region(rect(start, y, x - start, 1), region)
    x += 3
  }
}
