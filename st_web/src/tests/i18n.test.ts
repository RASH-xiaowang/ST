import { describe, expect, it } from "vitest";
import {
  isLocale,
  fallbackLocale,
  pick,
  LOCALES,
} from "@/lib/i18n/locales";

describe("i18n 核心", () => {
  it("识别合法 locale", () => {
    expect(isLocale("zh")).toBe(true);
    expect(isLocale("en")).toBe(true);
    expect(isLocale("fr")).toBe(false);
    expect(isLocale(undefined)).toBe(false);
  });

  it("非法 locale 回退 zh", () => {
    expect(fallbackLocale("jp")).toBe("zh");
    expect(fallbackLocale(null)).toBe("zh");
    expect(fallbackLocale("en")).toBe("en");
  });

  it("pick 按 locale 取值", () => {
    const bi = { zh: "你好", en: "Hello" };
    expect(pick(bi, "zh")).toBe("你好");
    expect(pick(bi, "en")).toBe("Hello");
  });

  it("候选语言列表完整", () => {
    expect(LOCALES).toEqual(["zh", "en"]);
  });
});
