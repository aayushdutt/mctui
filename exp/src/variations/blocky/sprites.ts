// One-row pixel art. A half block gives two square-ish pixels per cell (`▄`
// with fg = the bottom pixel and bg = the top one; `▀` alone when the bottom
// is see-through), so an N-cell sprite is N×2 pixels.
// Sprites stay one row tall: taller ones would stripe on terminals with line
// spacing. Game colours live here and only ever paint pixels, never UI text.

import type { Ctx } from '../../kit'
import { mix, luminance } from '../../term/theme'
import type { Color } from '../../term/buffer'

/** Pixel colours, keyed by one letter. `.` is transparent. */
const RAW: Record<string, Color> = {
  G: '#6cae45', // grass
  g: '#4f8a33',
  D: '#8c5b3b', // dirt
  d: '#6b4329',
  S: '#8e8e8e', // stone
  s: '#686868',
  C: '#63e2d6', // diamond
  c: '#2ea79c',
  Y: '#f0cd3a', // gold
  y: '#c39419',
  R: '#d2402f', // redstone, tnt
  r: '#932718',
  W: '#e9e9e9', // snow, wool, glass
  w: '#b9b9b9',
  K: '#2b2b2b', // coal, creeper face
  P: '#b98b53', // planks
  p: '#8d6636',
  L: '#6c5031', // log
  B: '#4277d6', // water, lapis
  b: '#2b519e',
  O: '#e07b3a', // copper
  o: '#a95728',
  M: '#a86cd8', // amethyst
  m: '#7a47a3',
  E: '#41cf6c', // emerald
  e: '#24954b',
  A: '#dccf97', // sand
  a: '#bfae76',
  F: '#eea4c5', // cherry
  f: '#c7789c',
  N: '#9a4a3a', // brick
  n: '#6f2f24',
  H: '#4b3222', // hair
  T: '#c99a7b', // skin
  U: '#5a78d6', // eyes
  V: '#3f7d2b', // leaves
  Q: '#ece4d4', // bone, quartz
  I: '#d6d6d6', // iron
  Z: '#5fb34a', // creeper
  z: '#3d8a2f',
}

export type Sprite = readonly [top: string, bottom: string]

export const SPRITES = {
  // instances (4 wide)
  grass: ['GGgG', 'DgDd'],
  tnt: ['RrRR', 'WKKW'],
  bricks: ['NNwN', 'wNNn'],
  island: ['GGGG', '.DD.'],
  diamond: ['SCSs', 'sSCS'],
  cherry: ['FfFF', '.LL.'],
  creeper: ['KZZK', 'ZKKz'],
  mountain: ['BWWB', 'SsSS'],
  // mods (3 wide)
  fabric: ['AaA', 'aAa'],
  flame: ['.Y.', 'OYO'],
  sign: ['PpP', '.L.'],
  sunset: ['OYO', 'mMm'],
  dust: ['.R.', 'RrR'],
  eye: ['EKE', 'eEe'],
  spyglass: ['OoC', 'oo.'],
  glass: ['CWC', 'WCW'],
  wool: ['WwW', 'wWw'],
  arch: ['SSS', 'S.S'],
  gear: ['wSw', 'SwS'],
  jade: ['EeE', 'eEe'],
  table: ['pPp', 'PpP'],
  apple: ['RLe', 'RRR'],
  map: ['AGB', 'GBA'],
  ingot: ['.II', 'ww.'],
  torch: ['.Y.', '.L.'],
  peak: ['BWB', 'SsS'],
  chunk: ['GGG', 'DDD'],
  jukebox: ['KLK', 'PpP'],
  head: ['HHH', 'UTU'],
  shulker: ['MMM', 'mYm'],
  cake: ['WRW', 'PPP'],
  cog: ['YpY', 'pYp'],
  waystone: ['.M.', 'SSS'],
  pearl: ['eCe', 'CeC'],
  kotlin: ['MOB', 'OMB'],
  lamp: ['YOY', 'oYo'],
  // tabs and heads (2 wide), worlds (3 wide)
  chest: ['PK', 'pp'],
  redstone: ['R.', 'rR'],
  pick: ['CC', '.L'],
  flower: ['.Y', 'e.'],
  books: ['RB', 'PP'],
  world: ['GGg', 'DdD'],
  hardcore: ['R.R', '.R.'],
  castle: ['S.S', 'sSs'],
  sky: ['GGG', '.D.'],
  steve: ['HH', 'TT'],
  jeb: ['oo', 'TT'],
  notch: ['HH', 'TL'],
} as const satisfies Record<string, Sprite>

export type SpriteId = keyof typeof SPRITES

/** A pixel colour, eased toward the page so it sits in the theme. */
export function px(c: Ctx, key: string, fade = 0): Color | undefined {
  const raw = RAW[key]
  if (!raw) return undefined
  const { t } = c
  // Light pages: pull the colours in so they don't glare (and keep whites
  // from vanishing into the page). Dark: barely.
  let base = mix(raw, t.bg, t.light ? 0.22 : 0.06)
  if (t.light && luminance(base) > 0.6) base = mix(base, t.text, 0.22)
  return fade ? mix(base, t.bg, fade) : base
}

/**
 * Draws `s` at (x, y). Transparent pixels keep whatever is behind (a band,
 * a slot). `fade` pushes the whole thing toward the background (disabled).
 */
export function sprite(c: Ctx, x: number, y: number, s: Sprite | SpriteId, fade = 0): number {
  const [top, bot] = typeof s === 'string' ? SPRITES[s] : s
  for (let i = 0; i < top.length; i++) {
    const tc = px(c, top[i], fade)
    const bc = px(c, bot[i], fade)
    // `▄` over a bg rather than `▀`: font-drawn upper halves can leave a
    // sliver of the bg showing above them; lower halves sit flush.
    if (tc && bc) c.buf.set(x + i, y, '▄', { fg: bc, bg: tc })
    else if (tc) c.buf.set(x + i, y, '▀', { fg: tc })
    else if (bc) c.buf.set(x + i, y, '▄', { fg: bc })
  }
  return x + top.length
}

export const spriteWidth = (s: Sprite | SpriteId) => (typeof s === 'string' ? SPRITES[s] : s)[0].length
