// fzf-ish matching: the query's letters in order, anywhere; prefixes, word
// starts and runs score higher. Returns the matched positions to light up.

export interface Match {
  score: number
  hits: number[]
}

const SEP = /[\s\-_.·/():&]/

function wordStart(s: string[], i: number): boolean {
  if (i === 0) return true
  return SEP.test(s[i - 1]) || (/\D/.test(s[i - 1]) && /\d/.test(s[i]))
}

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
    if (hits.length < q.length) break
    let score = 0
    hits.forEach((p, k) => {
      score += 1 + (p === 0 ? 6 : wordStart(orig, p) ? 4 : 0)
      if (k > 0) score += p - hits[k - 1] === 1 ? 3 : -Math.min(3, (p - hits[k - 1]) * 0.3)
    })
    if (!best || score > best.score) best = { score, hits }
  }
  return best
}
