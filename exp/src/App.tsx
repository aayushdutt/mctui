import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react'
import { SCREENS, initialUi, type ScreenId, type Ui, type Variation } from './kit'
import { variations } from './registry'
import { live, liveGroups } from './live'
import { render } from './render'
import { key, click } from './keys'
import { lint } from './lint'
import { SCENARIOS, scenario } from './data/scenarios'
import type { Mock } from './data/mock'
import { Screen, type ScreenOptions } from './term/Screen'
import { PALETTES, themeByName, type Theme } from './term/theme'
import { parseAnsi, toAnsi } from './term/ansi'
import type { Buffer } from './term/buffer'

type Mode = 'variation' | 'compare' | 'live' | 'sheet'

interface Prefs {
  mode: Mode
  v: string
  s: ScreenId
  live: string
  theme: string
  size: string
  fs: number
  spacing: 'gap' | 'tight'
  font: string
  state: string
}

const SIZES = ['80x24', '100x30', '120x36', '140x40', '160x45']
const FONTS: Record<string, string> = {
  jetbrains: "'JetBrains Mono', ui-monospace, monospace",
  menlo: 'Menlo, ui-monospace, monospace',
  sf: "'SF Mono', ui-monospace, monospace",
}

const DEFAULTS: Prefs = {
  mode: 'variation',
  v: variations[0]?.id ?? 'baseline',
  s: 'home',
  live: 'home',
  theme: 'dark',
  size: '100x30',
  fs: 14,
  spacing: 'gap',
  font: 'jetbrains',
  state: 'idle',
}

function readHash(): Prefs {
  const q = new URLSearchParams(location.hash.slice(1))
  const p = { ...DEFAULTS } as Record<string, unknown>
  for (const k of Object.keys(DEFAULTS)) {
    const v = q.get(k)
    if (v !== null) p[k] = typeof DEFAULTS[k as keyof Prefs] === 'number' ? Number(v) : v
  }
  return p as unknown as Prefs
}

function usePrefs() {
  const [prefs, setPrefs] = useState(readHash)
  useEffect(() => {
    const onHash = () => setPrefs(readHash())
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])
  const update = useCallback((patch: Partial<Prefs>) => {
    setPrefs((p) => {
      const next = { ...p, ...patch }
      const q = new URLSearchParams()
      for (const [k, v] of Object.entries(next)) if (v !== DEFAULTS[k as keyof Prefs]) q.set(k, String(v))
      history.replaceState(null, '', '#' + q.toString())
      return next
    })
  }, [])
  return [prefs, update] as const
}

const parseSize = (s: string) => {
  const [cols, rows] = s.split('x').map(Number)
  return { cols: cols || 100, rows: rows || 30 }
}

export function App() {
  const [prefs, update] = usePrefs()
  const variation = variations.find((v) => v.id === prefs.v) ?? variations[0]
  const theme = useMemo(() => themeByName(prefs.theme), [prefs.theme])
  const data = useMemo(() => scenario(prefs.state), [prefs.state])
  const size = parseSize(prefs.size)
  const opts = (t: { fg: string; bg: string }, fs = prefs.fs): ScreenOptions => ({
    size: fs,
    lineHeight: prefs.spacing === 'gap' ? 1.45 : 1.2,
    gap: prefs.spacing === 'gap',
    font: FONTS[prefs.font] ?? FONTS.jetbrains,
    fg: t.fg,
    bg: t.bg,
  })

  // Shortcuts outside the terminal: [ ] direction, { } screen.
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if ((e.target as HTMLElement)?.closest?.('.term-focus, input, select')) return
      const i = variations.findIndex((v) => v.id === prefs.v)
      const si = SCREENS.findIndex((s) => s.id === prefs.s)
      if (e.key === ']') update({ v: variations[(i + 1) % variations.length].id, mode: 'variation' })
      else if (e.key === '[') update({ v: variations[(i - 1 + variations.length) % variations.length].id, mode: 'variation' })
      else if (e.key === '}') update({ s: SCREENS[(si + 1) % SCREENS.length].id })
      else if (e.key === '{') update({ s: SCREENS[(si - 1 + SCREENS.length) % SCREENS.length].id })
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [prefs.v, prefs.s, update])

  return (
    <div className="flex h-screen overflow-hidden bg-neutral-950 font-sans text-neutral-300">
      <Sidebar prefs={prefs} update={update} />
      <main className="flex min-w-0 flex-1 flex-col">
        <Toolbar prefs={prefs} update={update} />
        <div className="flex-1 overflow-auto">
          {prefs.mode === 'variation' && variation && (
            <VariationView
              key={`${variation.id}/${prefs.s}/${prefs.size}/${prefs.state}`}
              v={variation}
              data={data}
              screen={prefs.s}
              theme={theme}
              size={size}
              opts={opts({ fg: theme.text, bg: theme.bg })}
              onGo={(s) => update({ s })}
            />
          )}
          {prefs.mode === 'compare' && (
            <Compare prefs={prefs} theme={theme} data={data} size={size} opts={(fs) => opts({ fg: theme.text, bg: theme.bg }, fs)} update={update} />
          )}
          {prefs.mode === 'live' && <LiveView name={prefs.live} opts={opts} />}
          {prefs.mode === 'sheet' && <Sheet opts={opts} update={update} />}
        </div>
      </main>
    </div>
  )
}

function Sidebar({ prefs, update }: { prefs: Prefs; update: (p: Partial<Prefs>) => void }) {
  const [open, setOpen] = useState<string | null>(prefs.mode === 'live' ? (live.find((s) => s.name === prefs.live)?.group ?? null) : null)
  const rounds = [...new Set(variations.filter((v) => !v.archived).map((v) => v.round ?? 1))].sort((a, b) => b - a)
  const groups: [string, Variation[]][] = [
    ...rounds.map((r): [string, Variation[]] => [r === 0 ? 'Reference' : `Round ${r}`, variations.filter((v) => !v.archived && (v.round ?? 1) === r)]),
    ['Archived', variations.filter((v) => v.archived)],
  ].filter(([, list]) => (list as Variation[]).length > 0) as [string, Variation[]][]
  return (
    <aside className="flex w-72 flex-none flex-col overflow-y-auto border-r border-white/5 bg-neutral-900/60">
      <div className="px-5 pt-5 pb-4">
        <div className="font-mono text-sm font-bold text-violet-400">mctui</div>
        <div className="text-xs text-neutral-500">design lab · local only</div>
      </div>
      {groups.map(([label, list]) => (
        <div key={label}>
          <SectionLabel>{label}</SectionLabel>
          <nav className="flex flex-col gap-0.5 px-2">
            {list.map((v) => {
          const on = prefs.mode === 'variation' && prefs.v === v.id
          return (
            <button
              key={v.id}
              onClick={() => update({ v: v.id, mode: 'variation' })}
              className={`rounded-md px-3 py-2 text-left transition ${on ? 'bg-violet-500/15 text-white' : 'hover:bg-white/5'}`}
            >
              <div className={`text-sm font-medium ${v.archived ? 'text-neutral-500' : ''}`}>{v.name}</div>
              <div className="line-clamp-2 text-xs text-neutral-500">{v.archived ?? v.blurb}</div>
            </button>
          )
        })}
          </nav>
        </div>
      ))}
      <nav className="flex flex-col gap-0.5 px-2">
        <button
          onClick={() => update({ mode: 'compare' })}
          className={`mt-1 rounded-md px-3 py-2 text-left text-sm transition ${prefs.mode === 'compare' ? 'bg-violet-500/15 text-white' : 'text-neutral-400 hover:bg-white/5'}`}
        >
          ⊞ Compare all
        </button>
      </nav>
      <SectionLabel>
        Live app{' '}
        <span className="font-normal normal-case text-neutral-600">· {live.length} frames</span>
      </SectionLabel>
      <nav className="flex flex-col px-2 pb-6">
        {live.length === 0 && (
          <p className="px-3 text-xs text-neutral-500">
            Run <code className="text-neutral-300">make screenshots</code> in the repo to see the real app here.
          </p>
        )}
        {live.length > 0 && (
          <button
            onClick={() => update({ mode: 'sheet' })}
            className={`rounded-md px-3 py-1.5 text-left text-sm transition ${prefs.mode === 'sheet' ? 'bg-violet-500/15 text-white' : 'text-neutral-400 hover:bg-white/5'}`}
          >
            ⊞ Contact sheet
          </button>
        )}
        {liveGroups.map((g) => (
          <div key={g}>
            <button
              onClick={() => setOpen((o) => (o === g ? null : g))}
              className="w-full rounded-md px-3 py-1.5 text-left text-sm text-neutral-400 hover:bg-white/5"
            >
              <span className={`mr-1 inline-block transition ${open === g ? 'rotate-90' : ''}`}>›</span> {g}
              <span className="ml-1 text-neutral-600">{live.filter((s) => s.group === g).length}</span>
            </button>
            {open === g && (
            <div className="ml-4 flex flex-col border-l border-white/5 pl-2">
              {live
                .filter((s) => s.group === g)
                .map((s) => (
                  <button
                    key={s.name}
                    onClick={() => update({ live: s.name, mode: 'live' })}
                    className={`rounded px-2 py-1 text-left font-mono text-xs transition ${prefs.mode === 'live' && prefs.live === s.name ? 'bg-violet-500/15 text-white' : 'text-neutral-500 hover:text-neutral-300'}`}
                  >
                    {s.name}
                  </button>
                ))}
            </div>
            )}
          </div>
        ))}
      </nav>
    </aside>
  )
}

const SectionLabel = ({ children }: { children: React.ReactNode }) => (
  <div className="px-5 pt-4 pb-2 text-[11px] font-semibold tracking-wider text-neutral-500 uppercase">{children}</div>
)

function Toolbar({ prefs, update }: { prefs: Prefs; update: (p: Partial<Prefs>) => void }) {
  const showScreens = prefs.mode === 'variation' || prefs.mode === 'compare'
  return (
    <header className="flex flex-none flex-wrap items-center gap-x-4 gap-y-2 border-b border-white/5 px-5 py-3 text-sm">
      {showScreens && (
        <Segmented
          value={prefs.s}
          options={SCREENS.map((s) => [s.id, s.label])}
          onChange={(s) => update({ s: s as ScreenId })}
        />
      )}
      <div className="ml-auto flex flex-wrap items-center gap-3">
        <Select label="State" value={prefs.state} options={SCENARIOS.map((x) => [x.id, x.label])} onChange={(state) => update({ state })} />
        <Select label="Theme" value={prefs.theme} options={PALETTES.map((p) => [p.name, p.name])} onChange={(theme) => update({ theme })} />
        <Select label="Size" value={prefs.size} options={SIZES.map((s) => [s, s.replace('x', ' × ')])} onChange={(size) => update({ size })} />
        <Select
          label="Font"
          value={prefs.font}
          options={[
            ['jetbrains', 'JetBrains Mono'],
            ['menlo', 'Menlo'],
            ['sf', 'SF Mono'],
          ]}
          onChange={(font) => update({ font })}
        />
        <label className="flex items-center gap-2 text-neutral-500">
          <input
            type="range"
            min={9}
            max={20}
            value={prefs.fs}
            onChange={(e) => update({ fs: Number(e.target.value) })}
            className="w-20 accent-violet-400"
          />
          <span className="w-8 font-mono text-xs text-neutral-400">{prefs.fs}px</span>
        </label>
        <Segmented
          value={prefs.spacing}
          options={[
            ['gap', 'Line spacing'],
            ['tight', 'Tight'],
          ]}
          onChange={(spacing) => update({ spacing: spacing as Prefs['spacing'] })}
          title="Line spacing = your terminal: taller rows, backgrounds and vertical strokes don't fill the gap. Tight = terminals like Ghostty or kitty at default settings."
        />
      </div>
    </header>
  )
}

function Segmented({
  value,
  options,
  onChange,
  title,
}: {
  value: string
  options: [string, string][]
  onChange: (v: string) => void
  title?: string
}) {
  return (
    <div className="flex rounded-lg bg-white/5 p-0.5" title={title}>
      {options.map(([v, label]) => (
        <button
          key={v}
          onClick={() => onChange(v)}
          className={`rounded-md px-3 py-1 text-sm transition ${v === value ? 'bg-neutral-700 text-white shadow' : 'text-neutral-400 hover:text-neutral-200'}`}
        >
          {label}
        </button>
      ))}
    </div>
  )
}

function Select({ label, value, options, onChange }: { label: string; value: string; options: [string, string][]; onChange: (v: string) => void }) {
  return (
    <label className="flex items-center gap-2 text-neutral-500">
      {label}
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-md border border-white/10 bg-neutral-900 px-2 py-1 text-neutral-200 outline-none focus:border-violet-400/60"
      >
        {options.map(([v, l]) => (
          <option key={v} value={v}>
            {l}
          </option>
        ))}
      </select>
    </label>
  )
}

/** A terminal-window chrome around a frame. */
function Window({ title, children, bg, onClick }: { title: string; children: React.ReactNode; bg: string; onClick?: () => void }) {
  return (
    <div
      className={`inline-block overflow-hidden rounded-xl shadow-2xl ring-1 ring-white/10 ${onClick ? 'cursor-pointer transition hover:ring-violet-400/50' : ''}`}
      style={{ background: bg }}
      onClick={onClick}
    >
      <div className="flex items-center gap-2 border-b border-white/5 bg-black/20 px-3 py-2">
        <span className="h-2.5 w-2.5 rounded-full bg-red-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-amber-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-emerald-400/70" />
        <span className="ml-2 truncate font-mono text-[11px] text-neutral-500">{title}</span>
      </div>
      <div className="p-3">{children}</div>
    </div>
  )
}

function VariationView({
  v,
  data,
  screen,
  theme,
  size,
  opts,
  onGo,
}: {
  v: Variation
  data: Mock
  screen: ScreenId
  theme: Theme
  size: { cols: number; rows: number }
  opts: ScreenOptions
  onGo: (s: ScreenId) => void
}) {
  const [ui, setUi] = useState<Ui>(initialUi)
  const { buf, meta } = useMemo(() => render(v, screen, size, theme, ui, data), [v, screen, size.cols, size.rows, theme, ui, data])
  const frame = useRef<HTMLDivElement>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!meta.animated) return
    const id = setInterval(() => setUi((u) => ({ ...u, tick: u.tick + 1 })), 80)
    return () => clearInterval(id)
  }, [meta.animated])

  useEffect(() => frame.current?.focus(), [])

  const apply = (step: { ui: Ui; go?: ScreenId }) => (step.go ? onGo(step.go) : setUi(step.ui))

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return
    if (e.key === 'Shift') return
    e.preventDefault()
    apply(key(ui, e.key, e.shiftKey, meta, screen))
  }

  const cellAt = (e: MouseEvent) => {
    const el = frame.current?.querySelector('.term')
    if (!el) return null
    const r = el.getBoundingClientRect()
    const x = Math.floor(((e.clientX - r.left) / r.width) * buf.cols)
    const y = Math.floor(((e.clientY - r.top) / r.height) * buf.rows)
    return x >= 0 && y >= 0 && x < buf.cols && y < buf.rows ? { x, y } : null
  }

  const onMove = (e: MouseEvent) => {
    const cell = cellAt(e)
    const hover = cell ? buf.hit(cell.x, cell.y) : null
    if (hover !== ui.hover) setUi((u) => ({ ...u, hover }))
  }

  const onClick = (e: MouseEvent) => {
    const cell = cellAt(e)
    const region = cell && buf.hit(cell.x, cell.y)
    if (region) apply(click(ui, region, meta, screen, e.detail === 2))
  }

  const copy = async () => {
    await navigator.clipboard.writeText(toAnsi(buf))
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  const findings = useMemo(() => lint(buf), [buf])

  return (
    <div className="flex flex-col items-center gap-6 p-8">
      <div
        ref={frame}
        tabIndex={0}
        onKeyDown={onKeyDown}
        onMouseMove={onMove}
        onMouseLeave={() => ui.hover && setUi((u) => ({ ...u, hover: null }))}
        onClick={onClick}
        className="term-focus rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-violet-400/40"
      >
        <Window title={`${v.name} — ${screen} · ${size.cols}×${size.rows} · ${theme.name}`} bg={theme.bg}>
          <Screen buf={buf} o={opts} />
        </Window>
      </div>
      <div className="w-full max-w-4xl space-y-4">
        <div className="flex items-start justify-between gap-6">
          <div>
            <h1 className="text-lg font-semibold text-white">{v.name}</h1>
            <p className="mt-1 text-sm text-neutral-400">{v.blurb}</p>
          </div>
          <div className="flex flex-none gap-2">
            <button onClick={copy} className="rounded-md bg-white/5 px-3 py-1.5 text-xs text-neutral-300 hover:bg-white/10">
              {copied ? 'Copied ✓' : 'Copy as ANSI'}
            </button>
          </div>
        </div>
        {meta.error && <pre className="overflow-auto rounded-md bg-red-950/40 p-3 text-xs text-red-300">{meta.error}</pre>}
        {v.notes && v.notes.length > 0 && (
          <ul className="list-disc space-y-1 pl-5 text-sm text-neutral-400">
            {v.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        )}
        <div className="flex flex-wrap gap-2 text-xs">
          {findings.map((f) => (
            <span
              key={f.rule}
              title={f.detail}
              className={`rounded-full px-2.5 py-1 ${f.rule === 'colours' ? 'bg-white/5 text-neutral-400' : f.rule === 'stacked blocks' ? 'bg-sky-500/10 text-sky-300' : 'bg-amber-500/10 text-amber-300'}`}
            >
              {f.rule === 'colours' ? `${f.count} colours` : `${f.rule === 'stacked blocks' ? '·' : '▲'} ${f.rule} · ${f.count} cells`}
            </span>
          ))}
        </div>
        <p className="text-xs leading-relaxed text-neutral-600">
          Click the frame, then drive it: ↑↓ jk · tab panes · ←→ tabs · / search · ? help · : palette · x menu · d delete ·
          n m a s p screens · ↵ play · esc back. Outside the frame: [ ] direction, {'{ }'} screen. “Copy as ANSI” pastes into
          a real terminal: <code className="text-neutral-400">pbpaste | cat</code>.
        </p>
      </div>
    </div>
  )
}

function Compare({
  prefs,
  theme,
  data,
  size,
  opts,
  update,
}: {
  prefs: Prefs
  theme: Theme
  data: Mock
  size: { cols: number; rows: number }
  opts: (fs: number) => ScreenOptions
  update: (p: Partial<Prefs>) => void
}) {
  const [fs, setFs] = useState(8)
  return (
    <div className="p-8">
      <div className="mb-4 flex items-center gap-3 text-sm text-neutral-500">
        Every direction on <span className="text-neutral-300">{prefs.s}</span>. Click one to open it.
        <input type="range" min={5} max={14} value={fs} onChange={(e) => setFs(Number(e.target.value))} className="w-24 accent-violet-400" />
      </div>
      <div className="flex flex-wrap gap-6">
        {variations.filter((v) => !v.archived).map((v) => {
          const { buf } = render(v, prefs.s, size, theme, initialUi, data)
          return (
            <div key={v.id} className="flex flex-col gap-2">
              <Window title={v.name} bg={theme.bg} onClick={() => update({ v: v.id, mode: 'variation' })}>
                <Screen buf={buf} o={opts(fs)} />
              </Window>
              <div className="max-w-md text-xs text-neutral-500">
                <span className="font-medium text-neutral-300">{v.name}</span> — {v.blurb}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

const liveBg = (name: string) => (name.includes('light') ? { fg: '#27272a', bg: '#ffffff' } : { fg: '#e4e4e7', bg: '#18181b' })

function LiveView({ name, opts }: { name: string; opts: (t: { fg: string; bg: string }, fs?: number) => ScreenOptions }) {
  const shot = live.find((s) => s.name === name) ?? live[0]
  const buf: Buffer | null = useMemo(() => (shot ? parseAnsi(shot.text) : null), [shot])
  if (!shot || !buf) return <p className="p-8 text-sm text-neutral-500">No screenshots yet: run make screenshots.</p>
  const colors = liveBg(shot.name)
  return (
    <div className="flex flex-col items-center gap-4 p-8">
      <Window title={`live app — ${shot.name} · ${buf.cols}×${buf.rows}`} bg={colors.bg}>
        <Screen buf={buf} o={opts(colors)} />
      </Window>
      <p className="text-xs text-neutral-600">
        The real renderer’s output (target/screenshots/{shot.name}.ansi). Re-run make screenshots to refresh.
      </p>
    </div>
  )
}

function Sheet({ opts, update }: { opts: (t: { fg: string; bg: string }, fs?: number) => ScreenOptions; update: (p: Partial<Prefs>) => void }) {
  const [fs, setFs] = useState(6)
  const bufs = useMemo(() => live.map((s) => ({ s, buf: parseAnsi(s.text) })), [])
  return (
    <div className="p-8">
      <div className="mb-4 flex items-center gap-3 text-sm text-neutral-500">
        Every frame of the live app.
        <input type="range" min={4} max={12} value={fs} onChange={(e) => setFs(Number(e.target.value))} className="w-24 accent-violet-400" />
      </div>
      <div className="flex flex-wrap gap-5">
        {bufs.map(({ s, buf }) => (
          <div key={s.name} className="flex flex-col gap-1.5">
            <Window title={s.name} bg={liveBg(s.name).bg} onClick={() => update({ live: s.name, mode: 'live' })}>
              <Screen buf={buf} o={opts(liveBg(s.name), fs)} />
            </Window>
          </div>
        ))}
      </div>
    </div>
  )
}
