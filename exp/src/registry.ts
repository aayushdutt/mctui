// Every folder under src/variations with an index.ts is a direction.

import type { Variation } from './kit'

const found = import.meta.glob<{ default: Variation }>('./variations/*/index.ts', { eager: true })

export const variations: Variation[] = Object.values(found)
  .map((m) => m.default)
  .sort((a, b) => (a.order ?? 50) - (b.order ?? 50) || a.name.localeCompare(b.name))
