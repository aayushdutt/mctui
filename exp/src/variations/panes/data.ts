// Extra content the Panes direction needs to fill its always-visible panels:
// the Activity feed, more Modrinth hits for a dense table, loader builds for
// the wizard, and the machine's Java runtimes.

import type { LogLine, ModHit } from '../../data/mock'

export type ActivityKind = 'download' | 'queued' | 'ok' | 'info' | 'warn' | 'error'

export interface Activity {
  time: string
  kind: ActivityKind
  text: string
  /** Which instance it belongs to ('' = the launcher itself). */
  source: string
  /** 0..1 for downloads. */
  progress?: number
  /** Right-hand detail: sizes, durations. */
  detail?: string
}

/** Newest first. */
export const activity: Activity[] = [
  { time: '14:31', kind: 'download', text: 'Sodium 0.6.5', source: 'Survival 1.21', progress: 0.64, detail: '2.1 / 3.3 MB' },
  { time: '14:31', kind: 'queued', text: 'Java 25 runtime', source: 'Snapshot Test', detail: '48.2 MB' },
  { time: '14:20', kind: 'info', text: 'Survival 1.21 closed normally after 18m', source: 'Survival 1.21' },
  { time: '14:19', kind: 'error', text: 'Failed to download file: https://resources.download.minecraft.net/…', source: 'game log' },
  { time: '14:06', kind: 'warn', text: "Can't keep up! Is the server overloaded? Running 2143ms or 42 ticks behind", source: 'game log' },
  { time: '14:02', kind: 'info', text: 'Launched Survival 1.21 as Steve', source: 'Survival 1.21' },
  { time: '13:58', kind: 'ok', text: 'Installed Lithium 0.14.3', source: 'Survival 1.21' },
  { time: 'Sep 27', kind: 'warn', text: 'Skyblock Classic crashed on launch, exit code 1', source: 'Skyblock Classic' },
  { time: 'Sep 27', kind: 'ok', text: 'Created 26.1.2 Fabric with 5 mods', source: '26.1.2 Fabric' },
  { time: 'Sep 26', kind: 'ok', text: 'Signed in as Steve (Microsoft)', source: '' },
  { time: 'Sep 25', kind: 'ok', text: 'Updated Fabric API to 0.110.0', source: 'Survival 1.21' },
  { time: 'Sep 24', kind: 'ok', text: 'Downloaded Minecraft 1.21.4 (412 files)', source: 'Survival 1.21' },
  { time: 'Sep 24', kind: 'ok', text: 'Installed Sodium 0.6.0', source: 'Survival 1.21' },
  { time: 'Sep 22', kind: 'info', text: 'Snapshot 26.3-snapshot-4 is available', source: '' },
  { time: 'Sep 22', kind: 'ok', text: 'Created Snapshot Test', source: 'Snapshot Test' },
  { time: 'Sep 21', kind: 'info', text: 'Creative Build closed normally after 2h 10m', source: 'Creative Build' },
  { time: 'Sep 20', kind: 'ok', text: 'Enabled Faithful 32x and Vanilla Tweaks', source: 'Creative Build' },
  { time: 'Sep 19', kind: 'warn', text: 'jeb_ needs to sign in again: the Microsoft session expired', source: '' },
  { time: 'Sep 15', kind: 'info', text: 'Minecraft 26.3 is out', source: '' },
  { time: 'Sep 14', kind: 'ok', text: 'Java 21 runtime installed (arm64)', source: '' },
]

/** More Modrinth results, so the table has something to scroll at 140×40. */
export const moreHits: ModHit[] = [
  hit('FerriteCore', 'malte0811', '52.9M', 'Optimization', 'Client & server', 'Memory usage optimizations.', '3w ago'),
  hit('Cloth Config API', 'shedaniel', '112.4M', 'Library', 'Client & server', 'Configuration library for Minecraft mods.', '2mo ago'),
  hit('Indium', 'comp500', '31.6M', 'Library', 'Client-side', 'Sodium addon providing support for the Fabric Rendering API.', '1mo ago'),
  hit('ModernFix', 'embeddedt', '28.3M', 'Optimization', 'Client & server', 'All-in-one mod that improves performance, reduces memory usage, and fixes many bugs.', '6d ago'),
  hit('AppleSkin', 'squeek502', '47.8M', 'Utility', 'Client & server', 'Adds various food-related HUD improvements.', '5mo ago'),
  hit('Jade', 'Snownee', '44.1M', 'Utility', 'Client & server', "Shows information about what you're looking at.", '2w ago'),
  hit("Xaero's Minimap", 'xaero96', '58.5M', 'Adventure', 'Client-side', 'Displays a minimap of the explored world with waypoints.', '4d ago'),
  hit('Krypton', 'astei', '21.0M', 'Optimization', 'Client & server', 'A mod to optimize the Minecraft networking stack.', '7mo ago'),
  hit('Dynamic FPS', 'juliand665', '19.4M', 'Optimization', 'Client-side', 'Reduce resource usage while Minecraft is in the background, idle or on battery.', '2w ago'),
  hit('LambDynamicLights', 'LambdAurora', '18.2M', 'Decoration', 'Client-side', 'Adds dynamic lights to the game: torches in hand light up the world around you.', '1mo ago'),
  hit('YetAnotherConfigLib', 'isXander', '17.6M', 'Library', 'Client-side', 'A builder-based configuration library for Minecraft.', '3w ago'),
  hit('Sodium Extra', 'FlashyReese', '16.9M', 'Optimization', 'Client-side', 'A Sodium addon that adds features that shouldn’t be in Sodium.', '1mo ago'),
  hit('Chat Heads', 'dzwdz', '9.8M', 'Utility', 'Client-side', 'Adds player heads next to their names in chat.', '3mo ago'),
  hit('Noisium', 'Steveplays28', '6.1M', 'Optimization', 'Client & server', 'Optimises worldgen performance for a better gameplay experience.', '2mo ago'),
]

/** What a mod needs and its license, for the details panel. */
export const modFacts: Record<string, { needs: string; license: string }> = {
  'Fabric API': { needs: 'nothing', license: 'Apache-2.0' },
  Sodium: { needs: 'nothing', license: 'Polyform Shield' },
  'Iris Shaders': { needs: 'Sodium', license: 'LGPL-3.0' },
  'Mod Menu': { needs: 'Fabric API', license: 'MIT' },
  Lithium: { needs: 'nothing', license: 'LGPL-3.0' },
  Indium: { needs: 'Sodium, Fabric API', license: 'Apache-2.0' },
  'Sodium Extra': { needs: 'Sodium', license: 'LGPL-3.0' },
  Zoomify: { needs: 'YetAnotherConfigLib', license: 'LGPL-3.0' },
}

function hit(name: string, author: string, downloads: string, category: string, side: string, description: string, updated: string): ModHit {
  return { name, author, downloads, category, side, description, updated }
}

export const loaders: { name: 'Vanilla' | 'Fabric'; version?: string; note: string; latest?: boolean }[] = [
  { name: 'Vanilla', note: 'no mods' },
  { name: 'Fabric', version: '0.17.2', note: 'latest', latest: true },
  { name: 'Fabric', version: '0.17.1', note: '' },
  { name: 'Fabric', version: '0.16.14', note: '' },
  { name: 'Fabric', version: '0.16.10', note: '' },
]

export const javas: { name: string; detail: string; used: number }[] = [
  { name: 'Java 25', detail: 'arm64', used: 2 },
  { name: 'Java 21', detail: 'arm64', used: 1 },
  { name: 'Java 17', detail: 'arm64', used: 1 },
  { name: 'Java 8', detail: 'x64 · Rosetta', used: 1 },
]

/** Facts the wide details grid adds per instance. */
export const extra: Record<string, { created: string; launches: string; disk: string }> = {
  'Survival 1.21': { created: 'Aug 14', launches: '37', disk: '312 MB' },
  'Snapshot Test': { created: 'Sep 22', launches: '0', disk: '4 MB' },
  'Creative Build': { created: 'Mar 3', launches: '58', disk: '496 MB' },
  'Skyblock Classic': { created: 'Jan 9', launches: '112', disk: '31 MB' },
  '26.1.2 Fabric': { created: 'Jun 11', launches: '64', disk: '468 MB' },
}

/** The game kept running after the shared log's last line: more tail for 140×40. */
export const logTail: LogLine[] = [
  { time: '14:20:10', thread: 'Server thread', level: 'INFO', text: 'ThreadedAnvilChunkStorage (New World): All chunks are saved' },
  { time: '14:21:37', thread: 'Render thread', level: 'INFO', text: '[CHAT] Steve has made the advancement [Acquire Hardware]' },
  { time: '14:23:05', thread: 'Server thread', level: 'WARN', text: "Can't keep up! Is the server overloaded? Running 2051ms or 41 ticks behind" },
  { time: '14:25:10', thread: 'Server thread', level: 'INFO', text: "Saving chunks for level 'ServerLevel[New World]'/minecraft:the_nether" },
  { time: '14:25:11', thread: 'Server thread', level: 'INFO', text: 'Autosave finished in 412 ms' },
  { time: '14:27:48', thread: 'Render thread', level: 'INFO', text: '[CHAT] Steve has made the advancement [We Need to Go Deeper]' },
  { time: '14:29:02', thread: 'Render thread', level: 'INFO', text: 'Reloading ResourceManager: vanilla, fabric, sodium, lithium' },
  { time: '14:29:03', thread: 'Render thread', level: 'INFO', text: 'Created: 1024x512x4 minecraft:textures/atlas/blocks.png-atlas' },
  { time: '14:30:15', thread: 'Server thread', level: 'INFO', text: 'Saving chunks for level \'ServerLevel[New World]\'/minecraft:overworld' },
  { time: '14:30:16', thread: 'Server thread', level: 'INFO', text: 'Autosave finished in 388 ms' },
]
