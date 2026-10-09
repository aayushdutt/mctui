// Refined: the small crafted pieces every screen shares. Text is the
// material: alignment, weight and one accent, a sub-cell bar, lit matches.

import type { Ctx } from '../../kit'
import { type Line, type Style, charWidth, lineWidth, truncate, width, wrap } from '../../term/buffer'
import type { Instance, InstalledMod } from '../../data/mock'

// ── Sub-cell progress ────────────────────────────────────────────────────

const EIGHTHS = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█']

/** A one-row bar filled to 1/8 of a cell on a quiet track. */
export function bar(c: Ctx, x: number, y: number, w: number, ratio: number, fg = c.t.accent, track = c.t.surfaceHi) {
  const filled = Math.round(Math.max(0, Math.min(1, ratio)) * w * 8)
  for (let i = 0; i < w; i++) {
    const v = Math.max(0, Math.min(8, filled - i * 8))
    c.buf.set(x + i, y, EIGHTHS[v], { fg, bg: track })
  }
}

export const pct = (ratio: number) => `${Math.round(ratio * 100)}%`

// ── Fuzzy filter with lit letters ────────────────────────────────────────

/**
 * Where `q` matches in `s` (character positions), or null. A contiguous
 * match at a word start beats one mid-word, which beats a scattered one.
 */
export function fuzzy(q: string, s: string): { at: number[]; score: number } | null {
  if (!q) return { at: [], score: 0 }
  const ql = [...q.toLowerCase()]
  const sl = [...s.toLowerCase()]
  const needle = ql.join('')
  const hay = sl.join('')
  const range = (i: number) => ql.map((_, k) => i + k)
  const wordStart = (i: number) => i === 0 || /[\s\-_.:]/.test(sl[i - 1])
  for (let i = hay.indexOf(needle); i >= 0; i = hay.indexOf(needle, i + 1)) {
    if (wordStart(i)) return { at: range(i), score: i === 0 ? 3 : 2 }
  }
  const i = hay.indexOf(needle)
  if (i >= 0) return { at: range(i), score: 1 }
  const at: number[] = []
  let j = 0
  for (let k = 0; k < sl.length && j < ql.length; k++) if (sl[k] === ql[j]) (at.push(k), j++)
  return j === ql.length ? { at, score: 0 } : null
}

/** Filters and ranks `items` by `q` (stable within a score). */
export function filter<T>(items: T[], q: string, key: (t: T) => string): { item: T; at: number[] }[] {
  const out = items
    .map((item, i) => ({ item, i, m: fuzzy(q, key(item)) }))
    .filter((x) => x.m)
    .map((x) => ({ item: x.item, i: x.i, at: x.m!.at, score: x.m!.score }))
  if (q) out.sort((a, b) => b.score - a.score || a.i - b.i)
  return out.map(({ item, at }) => ({ item, at }))
}

/** Writes `s` cut to `max` cells, the matched characters lit in the accent. */
export function lit(c: Ctx, x: number, y: number, s: string, at: number[], st: Style, max: number): number {
  const shown = [...truncate(s, max)]
  const cut = shown.length < [...s].length || (shown.length && shown[shown.length - 1] === '…' && !s.endsWith('…'))
  const on = new Set(at)
  shown.forEach((ch, i) => {
    const hit = on.has(i) && !(cut && i === shown.length - 1)
    c.buf.set(x, y, ch, hit ? { ...st, fg: c.t.accent, bold: true, underline: true } : st)
    x += charWidth(ch)
  })
  return x
}

// ── Key hints ────────────────────────────────────────────────────────────

export type Hint = [key: string, label: string, region?: string]

/** `↵ play   n new   / filter`: keys bright, verbs quiet, no boxes. Returns the end x. */
export function hints(c: Ctx, x: number, y: number, list: Hint[], max = c.buf.cols - x, gap = 3): number {
  const { t, buf } = c
  const limit = x + max
  let first = true
  for (const [key, label, region] of list) {
    const need = (first ? 0 : gap) + width(key) + 1 + width(label)
    if (x + need > limit) break
    if (!first) x += gap
    first = false
    const start = x
    const reg = region ?? `key:${key}`
    const hot = c.hovered(reg)
    x = buf.text(x, y, key, { fg: hot ? t.accent : t.strong, bold: true })
    x = buf.text(x + 1, y, label, { fg: hot ? t.text : t.muted })
    buf.region({ x: start, y, w: x - start, h: 1 }, reg)
  }
  return x
}

export const hintsWidth = (list: Hint[], gap = 3) =>
  list.reduce((w, [k, l], i) => w + (i ? gap : 0) + width(k) + 1 + width(l), 0)

/** A single quiet affordance, `↵ play`: the key in the accent. */
export function affordance(c: Ctx, key: string, label: string): Line {
  return [
    [key, { fg: c.t.accent, bold: true }],
    [` ${label}`, { fg: c.t.text }],
  ]
}

// ── Data shaping ─────────────────────────────────────────────────────────

/** Minutes since a mock "2h ago" / "yesterday" / "3d ago"; never is last. */
export function ago(s: string | null): number {
  if (!s) return Number.POSITIVE_INFINITY
  if (s === 'just now' || s === 'now') return 0
  if (s === 'yesterday') return 60 * 24
  const m = s.match(/(\d+)\s*(mo|m|h|d|w|y)/)
  if (!m) return 1e9
  const unit: Record<string, number> = { m: 1, h: 60, d: 1440, w: 10080, mo: 43200, y: 525600 }
  return Number(m[1]) * unit[m[2]]
}

/** Most recently played first: the one you want is already selected. */
export const byRecent = (list: Instance[]) =>
  list
    .map((inst, i) => ({ inst, i }))
    .sort((a, b) => ago(a.inst.lastPlayed) - ago(b.inst.lastPlayed) || a.i - b.i)
    .map((x) => x.inst)

/** `26.3-snapshot-4` → `26.3-snap4`: the useful end kept. */
export const shortMc = (mc: string) => mc.replace(/-snapshot-(\d+)$/, '-snap$1')

export const loaderText = (i: Instance) => (i.loader === 'Vanilla' ? 'Vanilla' : `${i.loader} ${i.loaderVersion ?? ''}`.trim())

/** `Java 21 (automatic)` → `Java 21`. */
export const javaShort = (j: string) => j.replace(/\s*\(automatic\)/, '')

export type StateKind = 'running' | 'crashed' | 'trouble' | 'download' | 'queued' | 'new' | 'idle'

export interface InstState {
  kind: StateKind
  ratio?: number
  label?: string
  detail?: string
}

/** What's true about an instance right now, most urgent first. */
export function stateOf(c: Ctx, inst: Instance): InstState {
  const { data } = c
  if (data.running?.instance === inst.name) return { kind: 'running' }
  if (data.crash?.instance === inst.name) return { kind: 'crashed' }
  const task = data.tasks.find((t) => t.instance === inst.name)
  if (task && task.detail !== 'queued') return { kind: 'download', ratio: task.ratio, label: task.label, detail: task.detail }
  if (task) return { kind: 'queued', label: task.label }
  if (inst.trouble) return { kind: 'trouble' }
  if (inst.fresh) return { kind: 'new' }
  return { kind: 'idle' }
}

/** The running game's elapsed time, ticking: `18m 04s`. */
export function elapsed(c: Ctx, base = '18m'): { short: string; long: string } {
  c.spinner() // keep the lab animating while a game runs
  const mins = Number(base.match(/(\d+)m/)?.[1] ?? 0)
  const secs = 4 + Math.floor(c.ui.tick * 0.08)
  const m = mins + Math.floor(secs / 60)
  const s = secs % 60
  return { short: `${m}m`, long: `${m}m ${String(s).padStart(2, '0')}s` }
}

/**
 * The crash in one human sentence (fitted to `max` cells when it can be:
 * `0.112 or newer` becomes `0.112+` rather than wrapping), plus the cause.
 */
export function crashWords(reason: string, max = Infinity): { sentence: string; cause: string } {
  const [cause, human] = reason.split(/\s+—\s+/)
  let sentence = (human ?? reason).replace(/\.?$/, '.')
  if (width(sentence) > max) sentence = sentence.replace(/ or newer\./, '+.')
  return { sentence, cause: human ? cause : '' }
}

/** Recency as brightness: today reads brighter than last season. */
export function agoColor(c: Ctx, s: string | null) {
  const m = ago(s)
  return m <= 1440 ? c.t.subtle : m <= 10080 ? c.t.dim : c.t.muted
}

/** Which installed mod a crash blames, and what it needs: `Fabric API` → `0.112`. */
export function crashNeeds(reason: string | undefined, mods: InstalledMod[]): { mod: string; needs: string } | null {
  if (!reason) return null
  const m = reason.match(/needs (.+?) ([\d.]+) or newer/)
  if (!m) return null
  const mod = mods.find((x) => x.name === m[1])
  return mod ? { mod: mod.name, needs: m[2] } : null
}

/** `a · b · c +2`: as many names as fit in `max` cells. */
export function joinFit(names: string[], max: number, sep = ' · '): string {
  let out = ''
  for (let i = 0; i < names.length; i++) {
    const next = out ? out + sep + names[i] : names[i]
    const rest = names.length - i - 1
    const tail = rest ? ` +${rest}` : ''
    if (width(next + tail) > max) {
      const left = names.length - i
      return out ? `${out} +${left}` : truncate(names[i], max)
    }
    out = next
  }
  return out
}

/** `s` cut at a word or `:` boundary to fit `max`, with an ellipsis. */
export function cutAt(s: string, max: number): string {
  if (width(s) <= max) return s
  const head = truncate(s, max).slice(0, -1)
  const i = Math.max(head.lastIndexOf(' '), head.lastIndexOf(':'))
  return (i > max / 2 ? head.slice(0, i) : head) + '…'
}

/** `WORLDS  2`: an uppercase group label and a faint count. Returns the end x. */
export function label(c: Ctx, x: number, y: number, text: string, count?: number | string, focused = false) {
  const end = c.buf.text(x, y, text.toUpperCase(), { fg: focused ? c.t.accent : c.t.muted, bold: true })
  if (count !== undefined) return c.buf.text(end + 2, y, String(count), { fg: c.t.faint })
  return end
}

/**
 * Joins `parts` with ` · `, keeping the most important ones that fit in
 * `max` (lower `prio` first) and showing them in their given order.
 */
export function fitParts(c: Ctx, parts: { line: Line; prio: number }[], max: number): Line {
  const sep: Line = [['  ·  ', { fg: c.t.faint }]]
  const keep = new Set<number>()
  const order = parts.map((p, i) => ({ ...p, i })).sort((a, b) => a.prio - b.prio)
  const widthOf = (ids: Set<number>) =>
    parts.reduce((w, p, i) => (ids.has(i) ? w + lineWidth(p.line) : w), 0) + Math.max(0, ids.size - 1) * 5
  for (const p of order) {
    const next = new Set(keep).add(p.i)
    if (widthOf(next) <= max) keep.add(p.i)
  }
  const out: Line = []
  parts.forEach((p, i) => {
    if (!keep.has(i)) return
    if (out.length) out.push(...sep)
    out.push(...p.line)
  })
  return out
}

/** `s` in one line, or two lines split as evenly as the words allow. */
export function balance(s: string, max: number): string[] {
  if (width(s) <= max) return [s]
  const words = s.split(' ')
  let best: string[] | null = null
  for (let i = 1; i < words.length; i++) {
    const a = words.slice(0, i).join(' ')
    const b = words.slice(i).join(' ')
    if (width(a) > max || width(b) > max) continue
    if (!best || Math.max(width(a), width(b)) < Math.max(width(best[0]), width(best[1]))) best = [a, b]
  }
  return best ?? wrap(s, max).slice(0, 2)
}
