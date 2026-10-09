// Draws one screen of one variation into a fresh buffer. Shared by the lab
// and the `pnpm shot` CLI.

import { Buffer, rect } from './term/buffer'
import type { Theme } from './term/theme'
import { mock, type Mock } from './data/mock'
import type { Ctx, Enter, ScreenId, SelOpts, Ui, Variation } from './kit'

const SPIN = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']

/** What the screen told the lab while drawing (list lengths, animation). */
export interface Meta {
  /** The focused pane's list length. */
  items: number | null
  /** Every list a screen declared, by pane. */
  lists: Record<number, { n: number; grid?: number; across?: boolean; numbered?: boolean }>
  /** The overlays this variation draws (keys only open these). */
  overlays: string[]
  oitems: number | null
  panes: number
  tabs: number | null
  animated: boolean
  /** What ↵ does, if the screen said (see Ctx.enter). */
  enter?: ScreenId | Enter | null
  /** Keys the screen bound to actions (Ctx.bind). */
  binds: Record<string, Enter>
  error?: string
}

export function render(
  v: Variation,
  screen: ScreenId,
  size: { cols: number; rows: number },
  t: Theme,
  ui: Ui,
  data: Mock = mock,
): { buf: Buffer; meta: Meta } {
  const buf = new Buffer(size.cols, size.rows)
  const meta: Meta = {
    items: null,
    lists: {},
    overlays: v.overlays ?? ['help', 'palette', 'menu', 'confirm'],
    binds: {},
    oitems: null,
    panes: 1,
    tabs: null,
    animated: false,
  }
  const clamp = (n: number, len: number) => Math.max(0, Math.min(n, len - 1))
  const ctx: Ctx = {
    buf,
    t,
    ui,
    data,
    sel(n, opts) {
      const o: SelOpts = typeof opts === 'number' ? { pane: opts } : (opts ?? {})
      const pane = o.pane ?? ui.focus
      meta.lists[pane] = { n, grid: o.grid, across: o.across, numbered: o.numbered }
      if (pane !== ui.focus) return clamp(ui.sels[pane] ?? 0, n)
      meta.items = n
      return clamp(ui.sel, n)
    },
    osel(n) {
      meta.oitems = n
      return clamp(ui.osel, n)
    },
    panes(n) {
      meta.panes = n
      return ((ui.focus % n) + n) % n
    },
    tabs(n) {
      meta.tabs = n
      return clamp(ui.tab, n)
    },
    hovered: (key) => ui.hover === key,
    enter(target) {
      meta.enter = target
    },
    bind(key, action) {
      meta.binds[key] = action
    },
    spinner() {
      meta.animated = true
      return SPIN[ui.tick % SPIN.length]
    },
  }
  const draw = v.screens[screen]
  try {
    if (draw) draw(ctx)
    else placeholder(ctx, v, screen)
  } catch (e) {
    meta.error = e instanceof Error ? (e.stack ?? e.message) : String(e)
    buf.fill(buf.area())
    buf.text(2, 1, `${v.id}/${screen} threw:`, { fg: t.error, bold: true })
    String(e).split('\n').forEach((l, i) => buf.text(2, 3 + i, l, { fg: t.text }))
  }
  return { buf, meta }
}

function placeholder(c: Ctx, v: Variation, screen: ScreenId) {
  const { buf, t } = c
  const r = rect(0, Math.floor(buf.rows / 2) - 1, buf.cols, 1)
  buf.lineCenter(r, [[v.name, { fg: t.muted, bold: true }], [`  hasn't designed ${screen} yet`, { fg: t.faint }]])
}
