// A small fzf-ish matcher: the query's letters in order, preferring word
// starts and runs. Returns the matched letters' positions, or null.

const SEP = /[\s\-_.·/():]/

export function fuzzy(query: string, text: string): number[] | null {
  const q = [...query.toLowerCase()].filter((ch) => ch !== ' ')
  if (!q.length) return []
  const s = [...text].map((ch) => ch.toLowerCase())
  let best: { hits: number[]; score: number } | null = null
  for (let start = 0; start < s.length; start++) {
    if (s[start] !== q[0]) continue
    const hits = [start]
    for (let i = start + 1; i < s.length && hits.length < q.length; i++) if (s[i] === q[hits.length]) hits.push(i)
    if (hits.length < q.length) break
    let score = 0
    hits.forEach((p, k) => {
      if (p === 0 || SEP.test(s[p - 1])) score += 4
      if (k && p === hits[k - 1] + 1) score += 3
    })
    if (!best || score > best.score) best = { hits, score }
  }
  return best?.hits ?? null
}
