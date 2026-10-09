// A tiny fzf-ish matcher: the query's letters in order, anywhere in the text,
// scored so prefixes, word starts and runs rank first.

export interface Match {
  score: number
  /** Code-point indexes of the matched characters, for highlighting. */
  hits: number[]
}

const SEP = /[\s\-_.·/()“”"'[\]:]/

function wordStart(s: string[], i: number): boolean {
  if (i === 0) return true
  const prev = s[i - 1]
  const cur = s[i]
  if (SEP.test(prev)) return true
  if (/[a-z]/.test(prev) && /[A-Z]/.test(cur)) return true
  return /\D/.test(prev) && /\d/.test(cur)
}

function score(orig: string[], hits: number[]): number {
  let n = 0
  hits.forEach((p, k) => {
    n += 1
    if (p === 0) n += 6
    else if (wordStart(orig, p)) n += 4
    if (k > 0) {
      const gap = p - hits[k - 1] - 1
      n += gap === 0 ? 3 : -Math.min(3, gap * 0.3)
    }
  })
  return n - orig.length * 0.02
}

/** Matches `query` against `text`; null when some letter is missing. */
export function fuzzy(query: string, text: string): Match | null {
  const q = [...query.toLowerCase()].filter((ch) => ch !== ' ')
  if (!q.length) return { score: 0, hits: [] }
  const orig = [...text]
  const s = orig.map((ch) => ch.toLowerCase())
  let best: Match | null = null
  for (let start = 0; start < s.length; start++) {
    if (s[start] !== q[0]) continue
    const hits = [start]
    for (let i = start + 1; i < s.length && hits.length < q.length; i++) if (s[i] === q[hits.length]) hits.push(i)
    // Greedy from here failed, so every later start fails too.
    if (hits.length < q.length) break
    const m = { score: score(orig, hits), hits }
    if (!best || m.score > best.score) best = m
  }
  return best
}

/** Case-insensitive substring positions (the log filter is literal, like fzf's `'`). */
export function substring(query: string, text: string): number[] | null {
  const q = query.toLowerCase()
  if (!q) return []
  const at = text.toLowerCase().indexOf(q)
  if (at < 0) return null
  const before = [...text.slice(0, at)].length
  return [...q].map((_, i) => before + i)
}
