// How a list of pages is cut into files. Positions are 1-based, as shown on the tiles.

export type SplitRule =
  | { kind: "every"; n: number }
  | { kind: "ranges"; text: string }
  | { kind: "each" }
  | { kind: "selected"; starts: number[] };

export type Plan = { ok: true; groups: number[][] } | { ok: false; error: string };

const range = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => from + i);

export function planSplit(rule: SplitRule, total: number): Plan {
  if (total === 0) return { ok: false, error: "There are no pages to split." };
  switch (rule.kind) {
    case "every":
      return chunkEvery(rule.n, total);
    case "each":
      return chunkEvery(1, total);
    case "ranges":
      return parseRanges(rule.text, total);
    case "selected":
      return splitBefore(rule.starts, total);
  }
}

export function chunkEvery(n: number, total: number): Plan {
  if (!Number.isInteger(n) || n < 1) return { ok: false, error: "Enter a whole number of pages, 1 or more." };
  const groups: number[][] = [];
  for (let start = 1; start <= total; start += n) groups.push(range(start, Math.min(start + n - 1, total)));
  return { ok: true, groups };
}

/** "1-3, 5, 8-" → [[1,2,3],[5],[8..total]]. Each comma-separated part becomes one file. */
export function parseRanges(text: string, total: number): Plan {
  const parts = text.split(",").map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return { ok: false, error: "Enter page ranges, for example 1-3, 4-8." };
  const groups: number[][] = [];
  for (const part of parts) {
    const m = /^(\d+)?\s*(-)?\s*(\d+)?$/.exec(part);
    if (!m || (!m[1] && !m[3]) || (!m[2] && !m[1])) return { ok: false, error: `"${part}" is not a page or range.` };
    const from = m[1] ? Number(m[1]) : 1;
    const to = m[2] ? (m[3] ? Number(m[3]) : total) : from;
    if (from < 1 || to > total) return { ok: false, error: `"${part}" is outside pages 1–${total}.` };
    if (from > to) return { ok: false, error: `"${part}" runs backwards.` };
    groups.push(range(from, to));
  }
  return { ok: true, groups };
}

/** A new file starts at each of `starts`; pages before the first start form the first file. */
export function splitBefore(starts: number[], total: number): Plan {
  const cuts = [...new Set(starts)].filter((s) => s > 1 && s <= total).sort((a, b) => a - b);
  if (cuts.length === 0) return { ok: false, error: "Select the pages where each new file should start." };
  const bounds = [1, ...cuts, total + 1];
  return { ok: true, groups: bounds.slice(0, -1).map((b, i) => range(b, bounds[i + 1] - 1)) };
}

/** "Report.pdf" → "Report" */
export const baseName = (fileName: string) => fileName.replace(/\.pdf$/i, "");

/** Output names: prefix_1.pdf … zero-padded so they sort in order. */
export function partNames(prefix: string, count: number): string[] {
  const width = String(count).length;
  return Array.from({ length: count }, (_, i) => `${prefix}_${String(i + 1).padStart(width, "0")}.pdf`);
}
