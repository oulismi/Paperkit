import { describe, expect, it } from "vitest";
import { chunkEvery, parseRanges, partNames, planSplit, splitBefore } from "./splitPlan";

const groups = (p: ReturnType<typeof planSplit>) => (p.ok ? p.groups : p.error);

describe("chunkEvery", () => {
  it("cuts into fixed-size files with a shorter last one", () => {
    expect(groups(chunkEvery(2, 5))).toEqual([[1, 2], [3, 4], [5]]);
  });
  it("rejects zero and fractions", () => {
    expect(chunkEvery(0, 5).ok).toBe(false);
    expect(chunkEvery(1.5, 5).ok).toBe(false);
  });
});

describe("parseRanges", () => {
  it("reads single pages, ranges and open ends", () => {
    expect(groups(parseRanges("1-2, 4 ,6-", 7))).toEqual([[1, 2], [4], [6, 7]]);
    expect(groups(parseRanges("-2", 7))).toEqual([[1, 2]]);
  });
  it("explains what is wrong", () => {
    expect(groups(parseRanges("", 5))).toMatch(/Enter page ranges/);
    expect(groups(parseRanges("3-9", 5))).toMatch(/outside pages 1–5/);
    expect(groups(parseRanges("4-2", 5))).toMatch(/backwards/);
    expect(groups(parseRanges("a", 5))).toMatch(/not a page/);
    expect(groups(parseRanges("-", 5))).toMatch(/not a page/);
  });
});

describe("splitBefore", () => {
  it("starts a new file at each selected page", () => {
    expect(groups(splitBefore([4, 2], 5))).toEqual([[1], [2, 3], [4, 5]]);
  });
  it("ignores page 1 and needs at least one cut", () => {
    expect(splitBefore([1], 5).ok).toBe(false);
  });
});

describe("partNames", () => {
  it("zero-pads so files sort in order", () => {
    expect(partNames("Report", 10)[0]).toBe("Report_01.pdf");
    expect(partNames("Report", 2)).toEqual(["Report_1.pdf", "Report_2.pdf"]);
  });
});
