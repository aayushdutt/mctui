// Content this direction needs beyond the shared mock: an icon per instance
// and mod, a fuller "creative inventory" page of Modrinth hits, and a few chat
// lines so the game log reads like the in-game chat.

import type { Instance, ModHit, Mock } from '../../data/mock'
import type { SpriteId } from './sprites'

export const SPLASH = 'Now in Rust!'

const INSTANCE_ICONS: Record<string, SpriteId> = {
  'Survival 1.21': 'grass',
  'Snapshot Test': 'tnt',
  'Creative Build': 'bricks',
  'Skyblock Classic': 'island',
  '26.1.2 Fabric': 'diamond',
}

export const instanceIcon = (i: Instance): SpriteId => INSTANCE_ICONS[i.name] ?? (i.loader === 'Fabric' ? 'diamond' : 'grass')

/** Icons a new instance can pick from (wizard). */
export const ICON_CHOICES: SpriteId[] = ['grass', 'diamond', 'tnt', 'bricks', 'island', 'cherry', 'creeper', 'mountain']

export const worldIcon = (mode: string, name: string): SpriteId =>
  mode === 'Hardcore' ? 'hardcore' : mode === 'Creative' ? 'castle' : name === 'Skyblock' ? 'sky' : 'world'

export const accountIcon = (name: string): SpriteId => (name === 'jeb_' ? 'jeb' : name === 'Notch' ? 'notch' : 'steve')

const MOD_ICONS: Record<string, SpriteId> = {
  'Fabric API': 'fabric',
  Sodium: 'flame',
  'Mod Menu': 'sign',
  'Iris Shaders': 'sunset',
  Lithium: 'dust',
  'Entity Culling': 'eye',
  Zoomify: 'spyglass',
  Continuity: 'glass',
  'Cloth Config': 'wool',
  Architectury: 'arch',
  YetAnotherConfigLib: 'gear',
  Jade: 'jade',
  'Roughly Enough Items': 'table',
  AppleSkin: 'apple',
  "Xaero's Minimap": 'map',
  FerriteCore: 'ingot',
  LambDynamicLights: 'torch',
  'Distant Horizons': 'peak',
  Bobby: 'chunk',
  'Simple Voice Chat': 'jukebox',
  'Chat Heads': 'head',
  'Shulker Box Tooltip': 'shulker',
  "Farmer's Delight": 'cake',
  Create: 'cog',
  Waystones: 'waystone',
  Krypton: 'pearl',
  'Fabric Language Kotlin': 'kotlin',
}

export const modIcon = (name: string): SpriteId => MOD_ICONS[name] ?? 'wool'

type Extra = [name: string, author: string, downloads: string, category: string, side: string, description: string, updated: string]

const EXTRA: Extra[] = [
  ['Cloth Config', 'shedaniel', '96.2M', 'Library', 'Client & server', 'Configuration library for Minecraft mods.', '3w ago'],
  ['Architectury', 'shedaniel', '88.0M', 'Library', 'Client & server', 'An intermediary API aimed at easing multi-loader mods.', '1mo ago'],
  ['FerriteCore', 'malte0811', '71.5M', 'Optimization', 'Client & server', 'Memory usage optimizations.', '2w ago'],
  ['YetAnotherConfigLib', 'isXander', '58.9M', 'Library', 'Client-side', 'A builder-based configuration library for Minecraft.', '5d ago'],
  ['Jade', 'Snownee', '57.3M', 'Utility', 'Client & server', 'Shows information about what you are looking at.', '4d ago'],
  ['Roughly Enough Items', 'shedaniel', '55.1M', 'Utility', 'Client & server', 'Browse items and recipes, the way you like it.', '1w ago'],
  ['AppleSkin', 'squeek502', '52.8M', 'Utility', 'Client & server', 'Adds food value and saturation to the hunger bar.', '2mo ago'],
  ["Xaero's Minimap", 'xaero96', '49.6M', 'Utility', 'Client-side', 'Displays a minimap of the surrounding area.', '6d ago'],
  ['LambDynamicLights', 'LambdAurora', '44.9M', 'Decoration', 'Client-side', 'Held torches light up the world around you.', '3w ago'],
  ['Distant Horizons', 'jeseibel', '37.4M', 'Optimization', 'Client-side', 'See much further without turning your game into a slideshow.', '2w ago'],
  ['Bobby', 'Johni0702', '26.0M', 'Optimization', 'Client-side', 'Keeps chunks around so render distance beats the server’s.', '1mo ago'],
  ['Simple Voice Chat', 'henkelmax', '31.7M', 'Utility', 'Client & server', 'Proximity voice chat.', '3d ago'],
  ['Chat Heads', 'dzwdz', '18.2M', 'Decoration', 'Client-side', 'Shows player heads next to chat messages.', '2mo ago'],
  ['Shulker Box Tooltip', 'MisterPeModder', '15.9M', 'Utility', 'Client-side', 'Peek inside shulker boxes from their tooltip.', '4mo ago'],
  ["Farmer's Delight", 'vectorwing', '24.6M', 'Adventure', 'Client & server', 'Cooking and farming, with a pinch of charm.', '1mo ago'],
  ['Create', 'simibubi', '33.3M', 'Technology', 'Client & server', 'Aesthetic technology: contraptions, cogs and conveyors.', '2w ago'],
  ['Waystones', 'BlayTheNinth', '29.8M', 'Adventure', 'Client & server', 'Teleport back to activated waystones.', '1w ago'],
  ['Krypton', 'astei', '21.4M', 'Optimization', 'Client & server', 'Optimizes the networking stack.', '5mo ago'],
  ['Fabric Language Kotlin', 'modmuss50', '47.0M', 'Library', 'Client & server', 'Lets mods be written in Kotlin.', '1w ago'],
]

/** A page of Modrinth hits: the shared eight, then more to fill the grid. */
export function modPage(data: Mock): ModHit[] {
  return [
    ...data.modHits,
    ...EXTRA.map(([name, author, downloads, category, side, description, updated]) => ({
      name,
      author,
      downloads,
      category,
      side,
      description,
      updated,
    })),
  ]
}

/** Creative-inventory tabs: the Modrinth categories with an icon each. */
export const CATEGORIES: [label: string, icon: SpriteId, match: string | null][] = [
  ['All', 'chest', null],
  ['Optimization', 'redstone', 'Optimization'],
  ['Utility', 'pick', 'Utility'],
  ['Decoration', 'flower', 'Decoration'],
  ['Library', 'books', 'Library'],
]

/** Chat lines woven into the log (the game prints them as `[CHAT] …`). */
export const CHAT: { time: string; text: string }[] = [
  { time: '14:08:40', text: '<Steve> found a village on the other side of the river' },
  { time: '14:15:12', text: '<Steve> diamonds!! at y -58' },
]
