import { describe, expect, it } from "vitest";
import { searchAll, searchIndex } from "@/lib/search";

describe("站内搜索", () => {
  it("索引覆盖全部内容域", () => {
    const idx = searchIndex("zh");
    const types = new Set(idx.map((e) => e.type));
    expect(types.has("doc")).toBe(true);
    expect(types.has("blog")).toBe(true);
    expect(types.has("faq")).toBe(true);
    expect(types.has("case")).toBe(true);
    expect(types.has("changelog")).toBe(true);
  });

  it("多词 AND 检索", () => {
    const hits = searchAll("工具 执行", "zh");
    expect(hits.length).toBeGreaterThan(0);
    for (const h of hits) {
      const hay = `${h.title} ${h.snippet} ${h.keywords}`.toLowerCase();
      expect(hay.includes("工具")).toBe(true);
      expect(hay.includes("执行")).toBe(true);
    }
  });

  it("英文检索同样工作", () => {
    const hits = searchAll("sandbox approval", "en", 10);
    expect(hits.length).toBeGreaterThan(0);
  });

  it("空查询返回空", () => {
    expect(searchAll("  ", "zh")).toEqual([]);
  });
});
