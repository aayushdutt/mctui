// How keys and clicks move the lab's Ui state: the app's grammar (DESIGN.md)
// in miniature, so mockups can be driven like the real thing.

import type { ScreenId, Ui } from './kit'
import type { Meta } from './render'

export interface Step {
  ui: Ui
  /** Switch to this screen (the Ui resets). */
  go?: ScreenId
}

const clamp = (n: number, len: number | null) => (len === null ? Math.max(0, n) : Math.max(0, Math.min(n, len - 1)))

/** Keyboard names as the app's keycaps print them. */
const CAP: Record<string, string> = {
  '↵': 'Enter',
  esc: 'Escape',
  tab: 'Tab',
  space: ' ',
  '⌫': 'Backspace',
  '↑': 'ArrowUp',
  '↓': 'ArrowDown',
  '←': 'ArrowLeft',
  '→': 'ArrowRight',
}

export const capToKey = (cap: string) => CAP[cap] ?? cap

const NAV: Record<string, ScreenId> = { n: 'wizard', m: 'mods', a: 'accounts', s: 'settings', p: 'play' }

/** Moves focus to pane `f`, remembering the old pane's selection. */
export function focusPane(ui: Ui, f: number): Ui {
  if (f === ui.focus) return ui
  return { ...ui, focus: f, sels: { ...ui.sels, [ui.focus]: ui.sel }, sel: ui.sels[f] ?? 0 }
}

export function key(ui: Ui, k: string, shift: boolean, meta: Meta, screen: ScreenId): Step {
  const set = (patch: Partial<Ui>): Step => ({ ui: { ...ui, ...patch } })
  const list = meta.lists[ui.focus]
  const items = list?.n ?? meta.items
  const step = (d: number): Step => set({ sel: clamp(ui.sel + d, items) })
  const open = (overlay: string): Step =>
    meta.overlays.includes(overlay) ? set({ overlay, osel: 0, oquery: '' }) : { ui }

  if (ui.typing) {
    if (k === 'Escape') return set({ typing: false, query: ui.overlay ? ui.query : '' })
    if (k === 'Enter') return set({ typing: false })
    if (k === 'Backspace') return set({ query: ui.query.slice(0, -1), sel: 0 })
    if (k.length === 1) return set({ query: ui.query + k, sel: 0 })
  }

  // The palette takes typing straight away.
  if (ui.overlay === 'palette') {
    if (k === 'Backspace') return set({ oquery: ui.oquery.slice(0, -1), osel: 0 })
    if (k.length === 1 && k !== '?') return set({ oquery: ui.oquery + k, osel: 0 })
  }

  if (ui.overlay) {
    switch (k) {
      case 'Escape':
      case 'Enter':
        return set({ overlay: null, osel: 0, oquery: '' })
      case 'ArrowUp':
      case 'k':
        return set({ osel: clamp(ui.osel - 1, meta.oitems) })
      case 'ArrowDown':
      case 'j':
        return set({ osel: clamp(ui.osel + 1, meta.oitems) })
      case '?':
        return set({ overlay: ui.overlay === 'help' ? null : 'help' })
      case 'y':
        return set({ overlay: null })
    }
    return { ui }
  }

  const tabs = meta.tabs
  const bound = meta.binds[k]
  if (bound) return click(ui, bound, meta, screen, false)
  switch (k) {
    case 'ArrowUp':
    case 'k':
      return step(-(list?.grid ?? 1))
    case 'ArrowDown':
    case 'j':
      return step(list?.grid ?? 1)
    case 'PageUp':
      return step(-5 * (list?.grid ?? 1))
    case 'PageDown':
      return step(5 * (list?.grid ?? 1))
    case 'g':
    case 'Home':
      return set({ sel: 0 })
    case 'G':
    case 'End':
      return set({ sel: clamp(Number.MAX_SAFE_INTEGER, items) })
    case 'Tab':
      return { ui: focusPane(ui, (ui.focus + (shift ? -1 : 1) + meta.panes) % meta.panes) }
    case 'ArrowLeft':
    case 'h':
      if (list?.grid || list?.across) return step(-1)
      return tabs ? set({ tab: clamp(ui.tab - 1, tabs) }) : { ui: focusPane(ui, Math.max(0, ui.focus - 1)) }
    case 'ArrowRight':
    case 'l':
      if (list?.grid || list?.across) return step(1)
      return tabs ? set({ tab: clamp(ui.tab + 1, tabs) }) : { ui: focusPane(ui, Math.min(meta.panes - 1, ui.focus + 1)) }
    case '[':
      return tabs ? set({ tab: clamp(ui.tab - 1, tabs) }) : { ui }
    case ']':
      return tabs ? set({ tab: clamp(ui.tab + 1, tabs) }) : { ui }
    case '/':
      return set({ typing: true })
    case '?':
      return open('help')
    case ':':
      return open('palette')
    case 'x':
      return open('menu')
    case 'd':
    case 'Backspace':
      return open('confirm')
    case 'Enter': {
      const target = meta.enter === undefined ? (screen === 'home' ? 'play' : null) : meta.enter
      if (!target) return { ui }
      if (target.includes(':')) return target.startsWith('key:') ? { ui } : click(ui, target, meta, screen, false)
      return { ui, go: target as ScreenId }
    }
    case 'Escape':
      if (ui.query) return set({ query: '', sel: 0 })
      return screen === 'home' ? { ui } : { ui, go: 'home' }
  }
  if (/^[1-9]$/.test(k) && tabs) return set({ tab: clamp(Number(k) - 1, tabs) })
  if (/^[1-9]$/.test(k) && list?.numbered) return set({ sel: clamp(Number(k) - 1, items) })
  if (NAV[k] && NAV[k] !== screen) return { ui, go: NAV[k] }
  return { ui }
}

/** A click on region `region`. */
export function click(ui: Ui, region: string, meta: Meta, screen: ScreenId, double: boolean): Step {
  const [kind, arg = ''] = region.split(/:(.*)/s)
  const n = Number(arg)
  switch (kind) {
    case 'sel': {
      // `sel:N` picks row N of the focused pane; `sel:N@P` focuses pane P first.
      const [row, pane] = arg.split('@').map(Number)
      const base = pane === undefined || Number.isNaN(pane) ? ui : focusPane(ui, pane)
      const step = { ui: { ...base, sel: row } }
      return double ? key(step.ui, 'Enter', false, meta, screen) : step
    }
    case 'osel':
      return { ui: { ...ui, osel: n } }
    case 'focus':
      return { ui: focusPane(ui, n) }
    case 'tab': {
      // `tab:+1` / `tab:-1` step and wrap.
      if (/^[+-]/.test(arg) && meta.tabs) return { ui: { ...ui, tab: (((ui.tab + n) % meta.tabs) + meta.tabs) % meta.tabs } }
      return { ui: { ...ui, tab: n } }
    }
    case 'overlay':
      return { ui: { ...ui, overlay: arg, osel: 0 } }
    case 'close':
      return { ui: { ...ui, overlay: null, oquery: '' } }
    case 'screen':
      return { ui, go: arg as ScreenId }
    case 'query':
      return { ui: { ...ui, typing: true } }
    case 'key':
      return key(ui, capToKey(arg), false, meta, screen)
  }
  return { ui }
}
