import { describe, expect, it } from "vitest";
import { docPages } from "@/lib/content/docs";
import { posts, postBySlug } from "@/lib/content/blog";
import { changelog, roadmap } from "@/lib/content/changelog";
import { faqs } from "@/lib/content/faq";
import { cases } from "@/lib/content/cases";
import { wechatInsights } from "@/lib/content/wechat";

describe("内容数据完整性（中英双语齐备）", () => {
  it("文档：slug 唯一且每个页面有分组/摘要/正文", () => {
    const slugs = docPages.map((d) => d.slug);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const d of docPages) {
      expect(d.sections.length).toBeGreaterThan(0);
      expect(d.summary.zh.length).toBeGreaterThan(0);
      expect(d.summary.en.length).toBeGreaterThan(0);
    }
  });

  it("博客：slug 唯一且双语正文非空", () => {
    const slugs = posts.map((p) => p.slug);
    expect(new Set(slugs).size).toBe(slugs.length);
    for (const p of posts) {
      expect(p.body.zh.length).toBeGreaterThan(0);
      expect(p.body.en.length).toBeGreaterThan(0);
      expect(postBySlug(p.slug)).toBe(p);
    }
  });

  it("更新日志按日期倒序且版本号唯一（仅当前版本带版本号）", () => {
    const dates = changelog.map((e) => e.date);
    expect([...dates].sort().reverse()).toEqual(dates);
    const versions = changelog.map((e) => e.version).filter((v): v is string => Boolean(v));
    expect(versions.length).toBeGreaterThan(0);
    expect(new Set(versions).size).toBe(versions.length);
  });

  it("路线图状态取值合法", () => {
    const allowed = ["done", "active", "planned"];
    for (const ph of roadmap) {
      for (const item of ph.items) {
        expect(allowed).toContain(item.status);
      }
    }
  });

  it("FAQ 类别字段合法", () => {
    for (const f of faqs) {
      expect(["product", "tech", "privacy"]).toContain(f.cat);
      expect(f.q.zh.length).toBeGreaterThan(0);
      expect(f.q.en.length).toBeGreaterThan(0);
    }
  });

  it("客户案例：分类合法且双语内容完整", () => {
    const cats = new Set(cases.map((c) => c.category));
    expect(cats.size).toBeGreaterThanOrEqual(3);
    for (const c of cases) {
      expect(c.detail.zh.length).toBeGreaterThanOrEqual(3);
      expect(c.detail.en.length).toBeGreaterThanOrEqual(3);
    }
  });

  it("微信洞察：三步流程、年度帧、AI 提问与隐私引擎双语齐备", () => {
    expect(wechatInsights.steps.items).toHaveLength(3);
    expect(wechatInsights.annual.frames).toHaveLength(8);
    expect(wechatInsights.insights.items.length).toBeGreaterThanOrEqual(5);
    for (const s of wechatInsights.steps.items) {
      expect(s.name.zh.length).toBeGreaterThan(0);
      expect(s.name.en.length).toBeGreaterThan(0);
      expect(s.desc.zh.length).toBeGreaterThan(0);
      expect(s.desc.en.length).toBeGreaterThan(0);
    }
    for (const f of wechatInsights.annual.frames) {
      expect(f.name.zh.length).toBeGreaterThan(0);
      expect(f.name.en.length).toBeGreaterThan(0);
      expect(f.hint.zh.length).toBeGreaterThan(0);
      expect(f.hint.en.length).toBeGreaterThan(0);
      expect(f.field.length).toBeGreaterThan(0);
      expect(f.visual.kind.length).toBeGreaterThan(0);
    }
    for (const item of wechatInsights.insights.items) {
      expect(item.name.zh.length).toBeGreaterThan(0);
      expect(item.name.en.length).toBeGreaterThan(0);
      expect(item.desc.zh.length).toBeGreaterThan(0);
      expect(item.desc.en.length).toBeGreaterThan(0);
    }
    expect(wechatInsights.sample.code.zh.length).toBeGreaterThan(0);
    expect(wechatInsights.sample.code.en.length).toBeGreaterThan(0);
    expect(wechatInsights.ask.bullets.length).toBeGreaterThanOrEqual(3);
    expect(wechatInsights.ask.sample.code.zh.length).toBeGreaterThan(0);
    expect(wechatInsights.ask.sample.code.en.length).toBeGreaterThan(0);
    expect(wechatInsights.privacy.facts.length).toBeGreaterThanOrEqual(4);
    expect(wechatInsights.privacy.engine.rows.length).toBeGreaterThanOrEqual(3);
    for (const f of wechatInsights.privacy.facts) {
      expect(f.title.zh.length).toBeGreaterThan(0);
      expect(f.title.en.length).toBeGreaterThan(0);
    }
  });
});
