import { describe, expect, it } from "vitest";
import type { Suggestion } from "../../lib/tauri";
import { mergeSuggestions } from "./items";

const s = (page: number, text: string, x0 = 10): Suggestion => ({
  page,
  text,
  label: "Email",
  category: "contact",
  source: "rule",
  areas: [{ page, x0, y0: 10, x1: x0 + 50, y1: 20 }],
});

describe("mergeSuggestions", () => {
  it("groups the same text across pages, ignoring case", () => {
    const items = mergeSuggestions([], [s(1, "a@b.co"), s(3, "A@B.co"), s(1, "c@d.co")], "pending");
    expect(items).toHaveLength(2);
    expect(items[0].pages).toEqual([1, 3]);
    expect(items[0].areas).toHaveLength(2);
  });

  it("does not duplicate boxes when the same results arrive twice", () => {
    const once = mergeSuggestions([], [s(1, "a@b.co")], "pending");
    const twice = mergeSuggestions(once, [s(1, "a@b.co")], "pending");
    expect(twice[0].areas).toHaveLength(1);
  });

  it("approving via a search upgrades an existing pending item", () => {
    const items = mergeSuggestions([], [s(1, "a@b.co")], "pending");
    const after = mergeSuggestions(items, [s(1, "a@b.co")], "approved");
    expect(after[0].status).toBe("approved");
    expect(items[0].status).toBe("pending");
  });
});

describe("sourceText", () => {
  it("describes AI confidence in words", async () => {
    const { sourceText } = await import("./items");
    expect(sourceText({ source: "ai", confidence: 0.91 })).toBe("AI, high confidence");
    expect(sourceText({ source: "ai", confidence: 0.65 })).toBe("AI, medium confidence");
    expect(sourceText({ source: "rule" })).toBe("rule match");
  });
});
