// What a design variation is, and what it gets to draw with.

import type { Buffer } from './term/buffer'
import type { Theme } from './term/theme'
import type { Mock } from './data/mock'

/** The screens a variation can design. Missing ones show a placeholder. */
export const SCREENS = [
  { id: 'home', label: 'Home' },
  { id: 'mods', label: 'Mods' },
  { id: 'wizard', label: 'New instance' },
  { id: 'play', label: 'Game log' },
  { id: 'accounts', label: 'Accounts' },
  { id: 'settings', label: 'Settings' },
] as const

export type ScreenId = (typeof SCREENS)[number]['id']

/**
 * Interaction state the lab keeps for the screen on show. Keys and the mouse
 * change it; a screen only reads it.
 *
 * Keys (see keys.ts): ↑↓/jk move `sel` · tab/shift+tab move `focus` ·
 * ←→/hl or 1-9 move `tab` (or `focus` when the screen has no tabs) · `/`
 * types into `query` · overlays: `?` help, `:` palette (typing goes to
 * `oquery`), `x` menu, `d`/⌫ confirm · screens: `n` wizard, `m` mods, `a` accounts, `s` settings,
 * `p` / ↵ on home: play · esc steps back.
 *
 * Clickable regions (`buf.region(rect, key)`) use the same grammar:
 * `sel:3` `osel:1` `focus:1` `tab:2` `overlay:help` `close` `screen:mods`
 * `query` `key:↵` (acts like pressing the key).
 * Any other key only lights up under the pointer (`ctx.hovered(key)`).
 */
export interface Ui {
  /** The selected row in the focused pane. */
  sel: number
  /** What each pane had selected when focus last left it. */
  sels: Record<number, number>
  /** The selected row inside an open overlay (palette, menu). */
  osel: number
  /** What's typed into the command palette (separate from the screen's `query`). */
  oquery: string
  focus: number
  tab: number
  hover: string | null
  overlay: string | null
  query: string
  typing: boolean
  /** Animation frame, advancing every 80 ms while a screen asks for it. */
  tick: number
}

export const initialUi: Ui = {
  sel: 0,
  sels: {},
  osel: 0,
  oquery: '',
  focus: 0,
  tab: 0,
  hover: null,
  overlay: null,
  query: '',
  typing: false,
  tick: 0,
}

export interface Ctx {
  buf: Buffer
  t: Theme
  ui: Ui
  data: Mock
  /**
   * The selected row of a list of `n`, clamped (and tells the lab the length).
   * Without a pane it's the focused pane's list. With one (`c.sel(n, 0)` or
   * `{ pane: 0 }`) it's that pane's selection whether or not it has focus, so
   * a details pane can follow the list while focus is elsewhere.
   * `grid: k` lays the list out k per row (↑↓ jump k, ←→ step 1);
   * `across: true` makes ←→ step through it too (a horizontal shelf).
   */
  sel(n: number, opts?: number | SelOpts): number
  /** Like `sel`, for a list inside an overlay (regions `osel:N`). */
  osel(n: number): number
  /** How many panes `focus` cycles through (default 2). Returns the focused one. */
  panes(n: number): number
  /** How many tabs `tab` cycles through. Returns the active one. */
  tabs(n: number): number
  /** Whether the pointer is over region `key`. */
  hovered(key: string): boolean
  /** The spinner frame; asking for it keeps the lab animating. */
  spinner(): string
  /**
   * What ↵ does right now: a screen id (`'play'`), any region key (`'tab:2'`,
   * `'focus:1'`, `'overlay:menu'`, `'screen:mods'`), or nothing (null).
   * Default on home: play.
   */
  enter(target: ScreenId | Enter | null): void
  /**
   * Binds a key on this screen to a region action, overriding the lab's
   * default for it: `c.bind('s', 'tab:+1')` cycles tabs, `c.bind('u', 'overlay:confirm')`.
   * `tab:+1` / `tab:-1` step and wrap.
   */
  bind(key: string, action: Enter): void
}

export interface SelOpts {
  pane?: number
  grid?: number
  across?: boolean
  /** 1–9 pick rows 1–9 of this list directly (when the screen has no tabs). */
  numbered?: boolean
}

/** A region key ↵ can act out (see the Ui grammar). */
export type Enter = `${'overlay' | 'tab' | 'focus' | 'screen' | 'sel' | 'key'}:${string}`

export type ScreenFn = (ctx: Ctx) => void

export interface Variation {
  id: string
  name: string
  /** One or two sentences: the idea, and what it trades away. */
  blurb: string
  /** Longer design notes shown under the frame. */
  notes?: string[]
  /** Sort key in the sidebar; lower first. */
  order?: number
  /** A palette this direction was designed for (else the lab's pick). */
  theme?: string
  /** Which brainstorm round it came from (sidebar grouping). */
  round?: number
  /** Set aside after review; listed under Archived, with why. */
  archived?: string
  /** The overlays it draws; the lab only opens these. Default: all four. */
  overlays?: ('help' | 'palette' | 'menu' | 'confirm')[]
  screens: Partial<Record<ScreenId, ScreenFn>>
}

export const defineVariation = (v: Variation) => v
