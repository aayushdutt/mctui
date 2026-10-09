// The app's palettes (crates/mctui/src/theme.rs), resolved the same way:
// foreground roles from the palette, tinted surfaces mixed from the terminal's
// real background.

import type { Color } from './buffer'

export interface Palette {
  name: string
  background: Color
  accent: Color
  secondary: Color
  strong: Color
  text: Color
  title: Color
  subtle: Color
  dim: Color
  muted: Color
  faint: Color
  rule: Color
  success: Color
  warning: Color
  warningSoft: Color
  error: Color
}

export interface Theme extends Omit<Palette, 'background'> {
  /** The terminal background the frame is drawn on. */
  bg: Color
  light: boolean
  /** Keycaps, chips and the hover band: a whisper above the background. */
  surface: Color
  surfaceHi: Color
  /** The focused selection band: the background tinted with the accent. */
  selection: Color
  accentHi: Color
  successBg: Color
  warningBg: Color
  errorBg: Color
  /** Text behind an open overlay. */
  backdrop: Color
  /** Black or white, whichever reads on the accent. */
  onAccent: Color
}

export const PALETTES: Palette[] = [
  {
    name: 'dark',
    background: '#18181b',
    accent: '#a78bfa',
    secondary: '#c4b5fd',
    strong: '#fafafa',
    text: '#e4e4e7',
    title: '#e4e4e7',
    subtle: '#a1a1aa',
    dim: '#8e8e97',
    muted: '#71717a',
    faint: '#5e5e66',
    rule: '#3f3f46',
    success: '#10b981',
    warning: '#fbbf24',
    warningSoft: '#fcd34d',
    error: '#ef4444',
  },
  {
    name: 'light',
    background: '#ffffff',
    accent: '#6d28d9',
    secondary: '#7c3aed',
    strong: '#18181b',
    text: '#27272a',
    title: '#3f3f46',
    subtle: '#52525b',
    dim: '#71717a',
    muted: '#8a8a93',
    faint: '#a1a1aa',
    rule: '#d4d4d8',
    success: '#047857',
    warning: '#b45309',
    warningSoft: '#78350f',
    error: '#dc2626',
  },
  {
    name: 'gruvbox',
    background: '#282828',
    accent: '#d3869b',
    secondary: '#83a598',
    strong: '#fbf1c7',
    text: '#ebdbb2',
    title: '#ebdbb2',
    subtle: '#d5c4a1',
    dim: '#bdae93',
    muted: '#a89984',
    faint: '#928374',
    rule: '#504945',
    success: '#98971a',
    warning: '#d79921',
    warningSoft: '#fabd2f',
    error: '#fb4934',
  },
  {
    name: 'catppuccin',
    background: '#1e1e2e',
    accent: '#cba6f7',
    secondary: '#89b4fa',
    strong: '#cdd6f4',
    text: '#cdd6f4',
    title: '#bac2de',
    subtle: '#a6adc8',
    dim: '#9399b2',
    muted: '#7f849c',
    faint: '#585b70',
    rule: '#45475a',
    success: '#a6e3a1',
    warning: '#f9e2af',
    warningSoft: '#f9e2af',
    error: '#f38ba8',
  },
  {
    name: 'dracula',
    background: '#282a36',
    accent: '#bd93f9',
    secondary: '#ff79c6',
    strong: '#ffffff',
    text: '#f8f8f2',
    title: '#f8f8f2',
    subtle: '#c8c8e0',
    dim: '#8b8fb0',
    muted: '#6272a4',
    faint: '#6272a4',
    rule: '#3a3d4d',
    success: '#50fa7b',
    warning: '#f1fa8c',
    warningSoft: '#f1fa8c',
    error: '#ff5555',
  },
  {
    name: 'nord',
    background: '#2e3440',
    accent: '#c6a3bf',
    secondary: '#88c0d0',
    strong: '#eceff4',
    text: '#e5e9f0',
    title: '#d8dee9',
    subtle: '#c0c8d4',
    dim: '#9aa3b2',
    muted: '#7b8494',
    faint: '#616e88',
    rule: '#434c5e',
    success: '#a3be8c',
    warning: '#ebcb8b',
    warningSoft: '#ebcb8b',
    error: '#bf616a',
  },
]

const parse = (c: Color): [number, number, number] => {
  const n = parseInt(c.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

const toHex = (r: number, g: number, b: number): Color =>
  '#' + [r, g, b].map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, '0')).join('')

/** `a` moved `t` of the way towards `b`. */
export function mix(a: Color, b: Color, t: number): Color {
  const [ar, ag, ab] = parse(a)
  const [br, bg, bb] = parse(b)
  return toHex(ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t)
}

export function luminance(c: Color): number {
  const lin = (v: number) => {
    const s = v / 255
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  }
  const [r, g, b] = parse(c)
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

export function contrast(a: Color, b: Color): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return (hi + 0.05) / (lo + 0.05)
}

/** Black or white, whichever reads better on `bg`. */
export function onColor(bg: Color): Color {
  return contrast(bg, '#000000') >= contrast(bg, '#ffffff') ? '#000000' : '#ffffff'
}

/** `palette` on `background` (or the background it was designed on). */
export function resolve(p: Palette, background?: Color): Theme {
  const bg = background ?? p.background
  const light = luminance(bg) > 0.4
  const [surface, surfaceHi, selection, tint] = light ? [0.06, 0.11, 0.13, 0.14] : [0.08, 0.14, 0.26, 0.18]
  const { background: _, ...roles } = p
  return {
    ...roles,
    bg,
    light,
    surface: mix(bg, p.text, surface),
    surfaceHi: mix(bg, p.text, surfaceHi),
    selection: mix(bg, p.accent, selection),
    accentHi: mix(p.accent, p.strong, 0.2),
    successBg: mix(bg, p.success, tint),
    warningBg: mix(bg, p.warning, tint),
    errorBg: mix(bg, p.error, tint),
    backdrop: mix(bg, p.text, 0.15),
    onAccent: onColor(p.accent),
  }
}

export function themeByName(name: string, background?: Color): Theme {
  return resolve(PALETTES.find((p) => p.name === name) ?? PALETTES[0], background)
}
