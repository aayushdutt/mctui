// Shared, realistic content for every variation: the same fixtures the app's
// snapshot tests use (Survival 1.21, Steve, Fabric API…), plus enough extra
// to fill a bigger terminal.

export type Loader = 'Vanilla' | 'Fabric'

export interface World {
  name: string
  lastPlayed: string
  mode: 'Survival' | 'Creative' | 'Hardcore'
  size: string
}

export interface InstalledMod {
  name: string
  version: string
  enabled: boolean
  update?: string
}

export interface Instance {
  name: string
  mc: string
  loader: Loader
  loaderVersion?: string
  lastPlayed: string | null
  playtime: string
  worlds: World[]
  mods: InstalledMod[]
  packs: string[]
  memory: string
  java: string
  /** What happened last time, if it went wrong. */
  trouble?: 'crashed' | "couldn't start"
  /** Brand new: never played. */
  fresh?: boolean
}

export interface Account {
  name: string
  kind: 'Microsoft' | 'Offline'
  status: 'active' | 'expired' | 'offline' | 'ok'
  uuid: string
}

export interface ModHit {
  name: string
  author: string
  downloads: string
  category: string
  side: string
  description: string
  updated: string
  installed?: boolean
}

export interface Version {
  id: string
  date: string
  latest?: boolean
  snapshot?: boolean
}

export interface LogLine {
  time: string
  thread: string
  level: 'INFO' | 'WARN' | 'ERROR'
  text: string
}

export interface Mock {
  account: Account
  accounts: Account[]
  instances: Instance[]
  modHits: ModHit[]
  modTotal: string
  versions: Version[]
  log: LogLine[]
  settings: { label: string; value: string; kind: 'choice' | 'toggle' | 'field'; group: string; placeholder?: boolean }[]
  news: { title: string; date: string }[]
  /** A game running right now (scenario `running`). Its instance is `instances.find(name)`. */
  running?: Running
  /** Background work in flight (scenario `downloading`). */
  tasks: Task[]
  /** The last launch ended badly and hasn't been looked at (scenario `crashed`). */
  crash?: Crash
  /** Which scenario this data is (see scenarios.ts). */
  scenario: string
}

export interface Running {
  instance: string
  elapsed: string
  since: string
  world: string
  warnings: number
  errors: number
  memory: string
}

export interface Task {
  label: string
  instance?: string
  /** 0..1 */
  ratio: number
  detail: string
}

export interface Crash {
  instance: string
  when: string
  exit: string
  reason: string
}

const survival: Instance = {
  name: 'Survival 1.21',
  mc: '1.21.4',
  loader: 'Fabric',
  loaderVersion: '0.16.10',
  lastPlayed: '2h ago',
  playtime: '4h 12m',
  worlds: [
    { name: 'New World', lastPlayed: '2h ago', mode: 'Survival', size: '184 MB' },
    { name: 'Hardcore Run', lastPlayed: '3d ago', mode: 'Hardcore', size: '62 MB' },
  ],
  mods: [
    { name: 'Fabric API', version: '0.110.0', enabled: true },
    { name: 'Lithium', version: '0.14.3', enabled: true },
    { name: 'Sodium', version: '0.6.0', enabled: true, update: '0.6.5' },
  ],
  packs: [],
  memory: '4 GB',
  java: 'Java 21 (automatic)',
}

export const mock: Mock = {
  account: { name: 'Steve', kind: 'Microsoft', status: 'active', uuid: '069a79f4-44e9-4726-a5be-fca90e38aaf5' },
  accounts: [
    { name: 'Steve', kind: 'Microsoft', status: 'active', uuid: '069a79f4-44e9-4726-a5be-fca90e38aaf5' },
    { name: 'jeb_', kind: 'Microsoft', status: 'expired', uuid: '853c80ef-3c37-49fd-aa49-938b674adae6' },
    { name: 'Notch', kind: 'Offline', status: 'offline', uuid: 'b50ad385-829d-3141-a216-7e7d7539ba7f' },
  ],
  instances: [
    survival,
    {
      name: 'Snapshot Test',
      mc: '26.3-snapshot-4',
      loader: 'Vanilla',
      lastPlayed: null,
      playtime: '0m',
      worlds: [],
      mods: [],
      packs: [],
      memory: '2 GB',
      java: 'Java 25 (automatic)',
      fresh: true,
    },
    {
      name: 'Creative Build',
      mc: '1.20.1',
      loader: 'Vanilla',
      lastPlayed: '3d ago',
      playtime: '11h 40m',
      worlds: [{ name: 'Castle Plots', lastPlayed: '3d ago', mode: 'Creative', size: '412 MB' }],
      mods: [],
      packs: ['Faithful 32x', 'Vanilla Tweaks'],
      memory: '3 GB',
      java: 'Java 17 (automatic)',
    },
    {
      name: 'Skyblock Classic',
      mc: '1.8.9',
      loader: 'Vanilla',
      lastPlayed: '2w ago',
      playtime: '31h 05m',
      worlds: [{ name: 'Skyblock', lastPlayed: '2w ago', mode: 'Survival', size: '8 MB' }],
      mods: [],
      packs: ['Vanilla Tweaks'],
      memory: '2 GB',
      java: 'Java 8 (Intel, Rosetta)',
      trouble: 'crashed',
    },
    {
      name: '26.1.2 Fabric',
      mc: '26.1.2',
      loader: 'Fabric',
      loaderVersion: '0.17.2',
      lastPlayed: 'yesterday',
      playtime: '19h 22m',
      worlds: [{ name: '11 jun 26', lastPlayed: 'yesterday', mode: 'Survival', size: '396 MB' }],
      mods: [
        { name: 'Fabric API', version: '0.128.1', enabled: true },
        { name: 'Sodium', version: '0.7.1', enabled: true },
        { name: 'Iris Shaders', version: '1.9.0', enabled: true },
        { name: 'Mod Menu', version: '15.0.0', enabled: true },
        { name: 'Zoomify', version: '2.14.2', enabled: false },
      ],
      packs: ['Fresh Animations'],
      memory: '6 GB',
      java: 'Java 25 (automatic)',
    },
  ],
  modHits: [
    {
      name: 'Fabric API',
      author: 'modmuss50',
      downloads: '262.7M',
      category: 'Library',
      side: 'Client & server',
      description:
        'Lightweight and modular API providing common hooks and intercompatibility measures utilized by mods using the Fabric toolchain.',
      updated: '6d ago',
      installed: true,
    },
    {
      name: 'Sodium',
      author: 'jellysquid3',
      downloads: '181.3M',
      category: 'Optimization',
      side: 'Client-side',
      description: 'The fastest and most compatible rendering optimization mod for Minecraft.',
      updated: '2w ago',
      installed: true,
    },
    {
      name: 'Mod Menu',
      author: 'Prospector',
      downloads: '104.5M',
      category: 'Utility',
      side: 'Client-side',
      description: 'Adds a mod menu to view the list of mods you have installed.',
      updated: '1mo ago',
    },
    {
      name: 'Iris Shaders',
      author: 'coderbot',
      downloads: '98.4M',
      category: 'Decoration',
      side: 'Client-side',
      description: 'A modern shader pack loader for Minecraft intended to be compatible with existing OptiFine shader packs.',
      updated: '3d ago',
    },
    {
      name: 'Lithium',
      author: 'jellysquid3',
      downloads: '76.1M',
      category: 'Optimization',
      side: 'Client & server',
      description: 'No-compromises game logic optimization mod.',
      updated: '5d ago',
      installed: true,
    },
    {
      name: 'Entity Culling',
      author: 'tr7zw',
      downloads: '64.0M',
      category: 'Optimization',
      side: 'Client-side',
      description: 'Using async path-tracing to hide Block-/Entities that are not visible.',
      updated: '1w ago',
    },
    {
      name: 'Zoomify',
      author: 'isXander',
      downloads: '41.2M',
      category: 'Utility',
      side: 'Client-side',
      description: 'A zoom mod with infinite customizability.',
      updated: '2mo ago',
    },
    {
      name: 'Continuity',
      author: 'PepperCode1',
      downloads: '38.7M',
      category: 'Decoration',
      side: 'Client-side',
      description: 'A Fabric mod that allows for efficient connected textures.',
      updated: '4mo ago',
    },
  ],
  modTotal: '5,056',
  versions: [
    { id: '26.3', date: 'Sep 15, 2026', latest: true },
    { id: '26.3-snapshot-4', date: 'Sep 22, 2026', snapshot: true },
    { id: '26.2', date: 'Jun 16, 2026' },
    { id: '26.1.2', date: 'Apr 21, 2026' },
    { id: '26.1', date: 'Mar 24, 2026' },
    { id: '1.21.11', date: 'Dec 9, 2025' },
    { id: '1.21.10', date: 'Oct 7, 2025' },
    { id: '1.21.9', date: 'Sep 30, 2025' },
    { id: '1.21.8', date: 'Jul 17, 2025' },
    { id: '1.21.7', date: 'Jun 30, 2025' },
    { id: '1.21.6', date: 'Jun 17, 2025' },
    { id: '1.21.5', date: 'Mar 25, 2025' },
    { id: '1.21.4', date: 'Dec 3, 2024' },
    { id: '1.20.1', date: 'Jun 12, 2023' },
    { id: '1.18.2', date: 'Feb 28, 2022' },
    { id: '1.12.2', date: 'Sep 18, 2017' },
    { id: '1.8.9', date: 'Dec 9, 2015' },
  ],
  log: [
    { time: '14:02:11', thread: 'main', level: 'INFO', text: 'Loading Minecraft 1.21.4 with Fabric Loader 0.16.10' },
    { time: '14:02:11', thread: 'main', level: 'INFO', text: 'Loading 41 mods:' },
    { time: '14:02:11', thread: 'main', level: 'INFO', text: '  - fabric-api 0.110.0+1.21.4' },
    { time: '14:02:11', thread: 'main', level: 'INFO', text: '  - lithium 0.14.3+mc1.21.4' },
    { time: '14:02:11', thread: 'main', level: 'INFO', text: '  - sodium 0.6.0+mc1.21.4' },
    { time: '14:02:12', thread: 'main', level: 'WARN', text: 'Mod sodium uses the deprecated mixin config option "minVersion"' },
    { time: '14:02:13', thread: 'Render thread', level: 'INFO', text: 'Environment: Environment[sessionHost=https://sessionserver.mojang.com]' },
    { time: '14:02:14', thread: 'Render thread', level: 'INFO', text: 'Setting user: Steve' },
    { time: '14:02:15', thread: 'Render thread', level: 'INFO', text: 'Backend library: LWJGL version 3.3.3' },
    { time: '14:02:16', thread: 'Render thread', level: 'INFO', text: 'OpenGL: Apple M3 Pro GL version 4.1 Metal - 89.3' },
    { time: '14:02:17', thread: 'Render thread', level: 'WARN', text: 'Missing sound for event: minecraft:item.goat_horn.play' },
    { time: '14:02:18', thread: 'Render thread', level: 'INFO', text: 'Reloading ResourceManager: vanilla, fabric, sodium, lithium' },
    { time: '14:02:21', thread: 'Worker-Main-4', level: 'INFO', text: 'Found unifont_all_no_pua-15.1.05.hex, loading' },
    { time: '14:02:23', thread: 'Render thread', level: 'INFO', text: 'Sound engine started' },
    { time: '14:02:24', thread: 'Render thread', level: 'INFO', text: 'Created: 1024x512x4 minecraft:textures/atlas/blocks.png-atlas' },
    { time: '14:02:26', thread: 'Server thread', level: 'INFO', text: 'Starting integrated minecraft server version 1.21.4' },
    { time: '14:02:26', thread: 'Server thread', level: 'INFO', text: 'Preparing start region for dimension minecraft:overworld' },
    { time: '14:02:29', thread: 'Server thread', level: 'INFO', text: 'Time elapsed: 2841 ms' },
    { time: '14:02:30', thread: 'Server thread', level: 'INFO', text: 'Steve[local:E:1c4a] logged in with entity id 112 at (-83.5, 71.0, 204.5)' },
    { time: '14:02:30', thread: 'Server thread', level: 'INFO', text: 'Steve joined the game' },
    { time: '14:06:41', thread: 'Server thread', level: 'WARN', text: "Can't keep up! Is the server overloaded? Running 2143ms or 42 ticks behind" },
    { time: '14:12:02', thread: 'Render thread', level: 'INFO', text: '[CHAT] Steve has made the advancement [Stone Age]' },
    { time: '14:19:55', thread: 'Render thread', level: 'ERROR', text: 'Failed to download file: https://resources.download.minecraft.net/…' },
    { time: '14:20:10', thread: 'Server thread', level: 'INFO', text: 'Saving chunks for level \'ServerLevel[New World]\'/minecraft:overworld' },
  ],
  settings: [
    { group: 'Appearance', label: 'Theme', value: 'auto', kind: 'choice' },
    { group: 'Game', label: 'Show snapshots', value: 'Off', kind: 'toggle' },
    { group: 'Game', label: 'Java', value: 'Automatic', kind: 'field', placeholder: true },
    { group: 'Game', label: 'JVM arguments', value: '-Xmx2G -Xms512M', kind: 'field' },
    { group: 'Account', label: 'Microsoft client ID', value: "mctui's own", kind: 'field', placeholder: true },
  ],
  news: [
    { title: 'Minecraft 26.3 is out', date: 'Sep 15' },
    { title: 'Snapshot 26.3-4', date: 'Sep 22' },
  ],
  tasks: [],
  scenario: 'idle',
}
