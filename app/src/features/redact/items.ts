// Groups occurrences of the same sensitive text into one reviewable item, the
// way the mockup lists "Elena Marlowe-Kade · p.1, 3, 11".
import type { Area, Category, Suggestion } from "../../lib/tauri";

export type Status = "pending" | "approved";

export interface Item {
  id: string;
  text: string;
  label: string;
  category: Category;
  source: Suggestion["source"];
  /** Highest model probability among the occurrences (AI items only). */
  confidence?: number;
  pages: number[];
  areas: Area[];
  status: Status;
}

const keyOf = (s: Suggestion) => `${s.category}|${s.label}|${s.text.toLowerCase()}`;

/** Adds suggestions to `items`, merging repeats of the same text into one item. */
export function mergeSuggestions(items: Item[], suggestions: Suggestion[], status: Status): Item[] {
  const out = items.map((i) => ({ ...i, pages: [...i.pages], areas: [...i.areas] }));
  const byKey = new Map(out.map((i) => [i.id, i]));
  for (const s of suggestions) {
    const id = s.source === "manual" ? `manual|${s.page}|${s.areas.map((a) => `${a.x0},${a.y0}`).join(";")}` : keyOf(s);
    let item = byKey.get(id);
    if (!item) {
      item = { id, text: s.text, label: s.label, category: s.category, source: s.source, pages: [], areas: [], status };
      if (s.confidence != null) item.confidence = s.confidence;
      byKey.set(id, item);
      out.push(item);
    } else if (status === "approved") {
      item.status = "approved";
    }
    if (s.confidence != null) item.confidence = Math.max(item.confidence ?? 0, s.confidence);
    for (const a of s.areas) {
      if (!item.areas.some((b) => b.page === a.page && b.x0 === a.x0 && b.y0 === a.y0 && b.x1 === a.x1 && b.y1 === a.y1)) {
        item.areas.push(a);
      }
    }
    if (!item.pages.includes(s.page)) item.pages.push(s.page);
    item.pages.sort((a, b) => a - b);
  }
  return out;
}

export const CATEGORY_NAMES: Record<Category, string> = {
  names: "Names",
  addresses: "Addresses",
  contact: "Contact",
  ids: "IDs & bank",
  dates: "Dates",
  custom: "Your searches",
};

export const SOURCE_NAMES: Record<Suggestion["source"], string> = {
  rule: "rule match",
  search: "text search",
  manual: "drawn box",
  ai: "AI",
};

/** "rule match", or for AI items "AI, high confidence". */
export function sourceText(item: Pick<Item, "source" | "confidence">) {
  if (item.source !== "ai" || item.confidence == null) return SOURCE_NAMES[item.source];
  return `AI, ${item.confidence >= 0.8 ? "high" : item.confidence >= 0.6 ? "medium" : "low"} confidence`;
}

export const pagesText = (pages: number[]) => `p.${pages.join(", ")}`;
