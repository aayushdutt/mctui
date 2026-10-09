// Blocky's components: the title (grass block + wordmark + splash), the
// hotbar footer, the XP bar, inventory slots, wide game-menu buttons and item
// tooltips. Everything is one row tall; colour comes from theme roles, and
// game colours only ever paint sprites.

import type { Ctx } from '../../kit'
import { type Line, type Rect, type Style, rect, right, bottom, centered, width } from '../../term/buffer'
import { mix, onColor } from '../../term/theme'
import { type Sprite, type SpriteId, sprite, spriteWidth } from './sprites'
import { accountIcon } from './extra'

/** `▀▀▀▀ mctui › Survival 1.21 › Mods` with the player's head flush right. */
export function header(c: Ctx, r: Rect, crumbs: string[] = []) {
  const { t, buf, data } = c
  let x = sprite(c, r.x, r.y, 'grass') + 1
  x = buf.text(x, r.y, 'mctui', { fg: t.strong, bold: true })
  crumbs.forEach((crumb, i) => {
    x = buf.text(x + 2, r.y, '›', { fg: t.faint })
    x = buf.text(x + 2, r.y, crumb, { fg: i === crumbs.length - 1 ? t.text : t.dim })
  })
  const acct = data.account
  const name: Line = acct.status === 'expired' ? [['▲ ', { fg: t.warning }], [acct.name, { fg: t.text }]] : [[acct.name, { fg: t.text }]]
  const nx = buf.lineRight(r, name)
  const hx = nx - 3
  sprite(c, hx, r.y, accountIcon(acct.name))
  buf.region(rect(hx, r.y, right(r) - hx, 1), 'screen:accounts')
  return x
}

/** The title screen's yellow splash, hung off the logo's lower right. */
export function splash(c: Ctx, x: number, y: number, text: string) {
  c.buf.text(x, y, text, { fg: c.t.warning, bold: true, italic: true })
}

/** A section label: `INSTANCES   5`, accent while its pane has focus. */
export function section(c: Ctx, x: number, y: number, label: string, count?: string | number, focused = false) {
  const { t, buf } = c
  const end = buf.text(x, y, label.toUpperCase(), { fg: focused ? t.accent : t.muted, bold: true })
  if (count === undefined) return end
  return buf.text(end + 3, y, String(count), { fg: t.muted })
}

export type Hint = [key: string, label: string]

/**
 * The footer as a hotbar: equal slots, centred, each holding a key. The
 * active slot (the screen's main action) wears the selected-slot frame.
 */
export function hotbar(c: Ctx, r: Rect, hints: Hint[], active = 0) {
  const { t, buf } = c
  const natural = (h: Hint) => width(h[0]) + 1 + width(h[1])
  // Equal slots like the game's; when that doesn't fit, snug ones; then fewer.
  const layout = (hs: Hint[]): number[] | null => {
    for (const pad of [4, 2]) {
      const eq = Math.max(...hs.map(natural)) + pad
      if (hs.length * eq + hs.length - 1 <= r.w) return hs.map(() => eq)
    }
    const snug = hs.map((h) => natural(h) + 2)
    return snug.reduce((a, b) => a + b, 0) + hs.length - 1 <= r.w ? snug : null
  }
  let shown = hints
  let widths = layout(shown)
  while (!widths && shown.length > 1) {
    shown = shown.slice(0, -1)
    widths = layout(shown)
  }
  if (!widths) return
  const total = widths.reduce((a, b) => a + b, 0) + widths.length - 1
  let x = r.x + Math.max(0, Math.floor((r.w - total) / 2))
  shown.forEach(([key, label], i) => {
    const region = `key:${key}`
    const on = i === active
    const hov = c.hovered(region)
    const slot = rect(x, r.y, widths![i], 1)
    buf.fill(slot, { bg: on ? t.selection : hov ? t.surfaceHi : t.surface })
    if (on) {
      buf.set(x - 1, r.y, '▐', { fg: t.accent })
      buf.set(right(slot), r.y, '▌', { fg: t.accent })
    }
    const lx = x + Math.floor((slot.w - natural([key, label])) / 2)
    const kx = buf.text(lx, r.y, key, { fg: on ? t.accent : t.strong, bold: true })
    buf.text(kx + 1, r.y, label, { fg: on ? t.strong : hov ? t.text : t.dim })
    buf.region(slot, region)
    x = right(slot) + 1
  })
}

/** The XP bar: green `━` with the level number sitting in the middle. */
export function xpbar(c: Ctx, r: Rect, ratio: number, level: string) {
  const { t, buf } = c
  ratio = Math.min(1, Math.max(0, ratio))
  const filled = Math.round(r.w * ratio)
  buf.hline(r.x, r.y, filled, { fg: t.success }, '━')
  buf.hline(r.x + filled, r.y, r.w - filled, { fg: t.rule }, '─')
  const lw = width(level) + 2
  const lx = r.x + Math.floor((r.w - lw) / 2)
  buf.text(lx, r.y, ` ${level} `, { fg: t.success, bold: true })
}

export interface SlotState {
  selected?: boolean
  hovered?: boolean
  /** A glyph in the slot's corner, where the stack count sits in the game. */
  badge?: [string, string]
  fade?: number
  region?: string
}

export const SLOT_W = 5

/** An inventory slot: the item's sprite on a one-row tile, a cell of padding each side. */
export function slot(c: Ctx, x: number, y: number, icon: SpriteId | Sprite | null, s: SlotState = {}, inner = 3): Rect {
  const { t, buf } = c
  const w = inner + 2
  const r = rect(x, y, w, 1)
  const bg = s.selected ? t.selection : s.hovered ? t.surfaceHi : t.surface
  buf.fill(r, { bg })
  if (icon) sprite(c, x + 1 + Math.floor((inner - spriteWidth(icon)) / 2), y, icon, s.fade)
  if (s.selected) {
    buf.set(x, y, '▐', { fg: t.accent })
    buf.set(x + w - 1, y, '▌', { fg: t.accent })
  } else if (s.badge) buf.set(x + w - 1, y, s.badge[0], { fg: s.badge[1], bold: true })
  if (s.region) buf.region(r, s.region)
  return r
}

export type ButtonKind = 'primary' | 'secondary' | 'danger'

/** A wide game-menu button: the label centred, the key quieter beside it. */
export function button(
  c: Ctx,
  r: Rect,
  label: string,
  key?: string,
  kind: ButtonKind = 'secondary',
  region?: string,
  focused = false,
  opts: { icon?: SpriteId; fg?: string } = {},
) {
  const { t, buf } = c
  const hov = !!region && c.hovered(region)
  const bg =
    kind === 'primary'
      ? hov ? t.accentHi : t.accent
      : kind === 'danger'
        ? hov ? mix(t.error, t.strong, 0.15) : t.error
        : focused ? t.selection : hov ? t.surfaceHi : t.surface
  const fg = kind === 'secondary' ? (hov || focused ? t.strong : t.text) : onColor(bg)
  const row = rect(r.x, r.y, r.w, 1)
  buf.fill(row, { bg })
  const line: Line = [[label, { fg: opts.fg ?? fg, bold: kind !== 'secondary' || focused }]]
  if (key) line.push([`  ${key}`, { fg: kind === 'secondary' ? t.muted : fg }])
  const iw = opts.icon ? spriteWidth(opts.icon) + 2 : 0
  const lx = buf.lineCenter(rect(row.x + iw, row.y, row.w - iw, 1), line)
  if (opts.icon) sprite(c, lx - iw, r.y, opts.icon)
  if (focused) {
    // The game's hovered-button outline, as far as one row allows.
    buf.set(r.x, r.y, '▐', { fg: t.accent })
    buf.set(right(r) - 1, r.y, '▌', { fg: t.accent })
  }
  if (region) buf.region(row, region)
  return row
}

/**
 * An item tooltip: a name in the accent, grey lore, and the source in italic
 * blue last, between two accent-tinted rules. Returns the rows used.
 */
export function tooltip(c: Ctx, x: number, y: number, w: number, lines: Line[]): number {
  const { t, buf } = c
  const edge: Style = { fg: mix(t.bg, t.accent, 0.55) }
  buf.hline(x, y, w, edge, '─')
  lines.forEach((l, i) => buf.line(x + 1, y + 1 + i, l, { fg: t.dim }, w - 2))
  buf.hline(x, y + lines.length + 1, w, edge, '─')
  return lines.length + 2
}

/** Tab chips with an icon each (the creative inventory's tabs). Regions `tab:N`. */
export function iconTabs(c: Ctx, r: Rect, items: [label: string, icon: SpriteId, count?: number][], active: number, focused: boolean) {
  const { t, buf } = c
  let x = r.x
  items.forEach(([label, icon, count], i) => {
    const on = i === active
    const hov = c.hovered(`tab:${i}`)
    const need = spriteWidth(icon) + 1 + width(label) + (count !== undefined ? width(String(count)) + 1 : 0) + 2
    if (x + need > right(r)) return
    const chip = rect(x, r.y, need, 1)
    if (on) buf.fill(chip, { bg: focused ? t.selection : t.surface })
    else if (hov) buf.fill(chip, { bg: t.surface })
    let cx = sprite(c, x + 1, r.y, icon, on || hov ? 0 : 0.35) + 1
    cx = buf.text(cx, r.y, label, { fg: on ? t.strong : hov ? t.text : t.dim, bold: on })
    if (count !== undefined) buf.text(cx + 1, r.y, String(count), { fg: on ? t.subtle : t.muted })
    buf.region(chip, `tab:${i}`)
    x = right(chip) + 1
  })
  return x
}

/**
 * Stat tiles: bold values over muted labels. Spread evenly when there's
 * room, snug when there isn't, wrapped as a last resort. Returns rows used.
 */
export function stats(c: Ctx, r: Rect, items: [value: string, label: string][]): number {
  const { t, buf } = c
  const nat = items.map(([v, l]) => Math.max(width(v), width(l)))
  const even = Math.floor(r.w / items.length)
  const fitsEven = nat.every((n) => n + 2 <= even)
  const snugTotal = nat.reduce((a, b) => a + b + 3, -3)
  let x = r.x
  let y = r.y
  items.forEach(([value, label], i) => {
    const tw = fitsEven ? even : nat[i] + 3
    if (!fitsEven && snugTotal > r.w && x + nat[i] > right(r)) {
      x = r.x
      y += 3
    }
    buf.text(x, y, value, { fg: t.strong, bold: true }, tw - 1)
    buf.text(x, y + 1, label, { fg: t.muted }, tw - 1)
    x += tw
  })
  return y - r.y + 2
}

/**
 * A game-menu screen over the dimmed page: the title centred over a rule, a
 * closing rule. Returns the body area. Clicking outside closes it.
 */
export function panel(c: Ctx, w: number, h: number, title: Line, opts: { danger?: boolean } = {}): Rect {
  const { t, buf } = c
  ghost(c)
  buf.region(buf.area(), 'close')
  const r = centered(buf.area(), w, h)
  buf.clear(rect(r.x - 2, r.y, r.w + 4, r.h))
  buf.lineCenter(rect(r.x, r.y, r.w, 1), title, { fg: opts.danger ? t.error : t.strong, bold: true })
  const edge: Style = { fg: mix(t.bg, opts.danger ? t.error : t.accent, 0.55) }
  buf.hline(r.x, r.y + 1, r.w, edge)
  buf.hline(r.x, bottom(r) - 1, r.w, edge)
  buf.region(r, 'card')
  return rect(r.x, r.y + 3, r.w, r.h - 5)
}

/** Dims the page behind an overlay; sprites fade further so they recede. */
export function ghost(c: Ctx) {
  const { t, buf } = c
  buf.fade(t.backdrop)
  const faint = mix(t.bg, t.backdrop, 0.45)
  for (const cell of buf.cells) if (cell.ch === '▄' || cell.ch === '▀') cell.st = { fg: faint }
}
