// Renders variations to ANSI + PNG without a browser:
//
//   pnpm shot <variation|all> [screen|all] [--theme dark] [--size 100x30]
//             [--ui '{"sel":2,"overlay":"help"}'] [--scenario running] [--text] [--no-png]
//
// Writes shots/<variation>-<screen>[-<suffix>].ansi and .png (via
// scripts/ansi2png.py, which draws like a terminal with line spacing). `cat`
// an .ansi file in a truecolor terminal to see the real thing.

import { readdirSync, writeFileSync, mkdirSync, existsSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { dirname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { render } from '../src/render.ts'
import { SCREENS, initialUi, type ScreenId, type Variation } from '../src/kit.ts'
import { themeByName } from '../src/term/theme.ts'
import { toAnsi } from '../src/term/ansi.ts'
import { lint } from '../src/lint.ts'
import { scenario } from '../src/data/scenarios.ts'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
const flag = (name: string) => {
  const i = args.indexOf(`--${name}`)
  if (i < 0) return undefined
  const v = args[i + 1]
  args.splice(i, 2)
  return v
}
const bool = (name: string) => {
  const i = args.indexOf(`--${name}`)
  if (i < 0) return false
  args.splice(i, 1)
  return true
}

const themeName = flag('theme') ?? 'dark'
const [cols, rows] = (flag('size') ?? '100x30').split('x').map(Number)
const uiPatch = JSON.parse(flag('ui') ?? '{}')
const suffix = flag('suffix')
const scen = flag('scenario') ?? 'idle'
const text = bool('text')
const noPng = bool('no-png')
const [which = 'all', screenArg = 'all'] = args

const dir = join(root, 'src/variations')
const ids = readdirSync(dir).filter((d) => existsSync(join(dir, d, 'index.ts')))
const pick = which === 'all' ? ids : which.split(',')
const screens: ScreenId[] = screenArg === 'all' ? SCREENS.map((s) => s.id) : (screenArg.split(',') as ScreenId[])

const out = join(root, 'shots')
mkdirSync(out, { recursive: true })
const theme = themeByName(themeName)

for (const id of pick) {
  if (!ids.includes(id)) {
    console.error(`no variation "${id}" (have: ${ids.join(', ')})`)
    process.exitCode = 1
    continue
  }
  const v: Variation = (await import(pathToFileURL(join(dir, id, 'index.ts')).href)).default
  for (const screen of screens) {
    if (!v.screens[screen] && screenArg === 'all') continue
    const { buf, meta } = render(v, screen, { cols, rows }, theme, { ...initialUi, ...uiPatch }, scenario(scen))
    const name = [id, screen, scen !== 'idle' && scen, themeName !== 'dark' && themeName, suffix].filter(Boolean).join('-')
    const ansi = join(out, `${name}.ansi`)
    writeFileSync(ansi, toAnsi(buf))
    let png = ''
    if (!noPng) {
      png = join(out, `${name}.png`)
      execFileSync('python3', [join(root, 'scripts/ansi2png.py'), ansi, png], {
        env: { ...process.env, BG: theme.bg.slice(1) },
      })
    }
    const found = lint(buf)
    const issues = found.filter((f) => f.rule === 'vertical strokes' || f.rule === 'multi-row tint').map((f) => `${f.rule} ×${f.count}`)
    const notes = found.filter((f) => f.rule === 'stacked blocks').map((f) => `${f.rule} ×${f.count}`)
    console.log(
      `${name}: ${png || ansi}${issues.length ? `  ▲ ${issues.join(', ')}` : ''}${notes.length ? `  · ${notes.join(', ')}` : ''}${meta.error ? '  ✗ THREW' : ''}`,
    )
    if (meta.error) console.log(meta.error)
    if (text) console.log(buf.toText().join('\n'))
  }
}
