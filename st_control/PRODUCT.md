# Product

<!-- impeccable:product-schema 1 -->

## Platform

web (rendered inside a Windows desktop shell: Tauri + WebView2; window 1600×1000). [inferred from code: src-tauri, tauri.conf.json]

## Stack

Svelte 5 + TypeScript + Vite, Tailwind CSS v4, shadcn-svelte (Bits UI), fancy-ui-svelte (FancyUI), Rust/Tauri backend. [confirmed from package.json / DESIGN.md]

## Users

Primary user is a Chinese-speaking individual operator (likely the owner/administrator) running this app on their own Windows machine as a personal control console. They use it to operate and inspect local data (WeChat messages, knowledge base, LLM calls, agents, automation, OCR). [inferred from app name "ST 控制台", single-user desktop design, personalization features; to be confirmed]

## Product Purpose

"ST 控制台" is a desktop control console that unifies LLM workflows, agent orchestration, automation, WeChat data management and analysis, knowledge base, OCR, and database/usage dashboards under one local-first interface. Success means a task can be started and monitored from one surface without leaving the app. [confirmed from DESIGN.md feature inventory]

## Positioning

Everything the user manages — LLM providers/models, agents, WeChat sync, automation, knowledge, OCR — reports into one live control plane with real backend state, instead of being scattered across separate tools. [confirmed from DESIGN.md sections 7-13]

## Operating Context

- Desktop-first Windows app; window min ~1600×1000; sidebar navigation + content panels.
- Frequent states: server running/starting/stopping/error, agent connect/disconnect, real-time message/event logs, live counters.
- Panels operate independently and keep running in background while hidden.
- Local-first: WeChat DB, knowledge base, LLM config and usage all live on this machine; sensitive data (WeChat) is core content.
- Personalization: background themes (10), text themes (8), fonts, and opacity are user-adjustable and must keep working. [confirmed from DESIGN.md §6]

## Capabilities and Constraints

- Must preserve: all existing panels/features, real data, IPC wiring, keyboard shortcuts (Ctrl+B sidebar, Ctrl+K search), window drag/controls, personalized tokens (`--app-*`), shadcn + FancyUI component systems.
- Module tokens (knowledge base `--kb-*`, WeChat `--wc-*`) are derived from `--app-*`; new colors must go through tokens, not hardcoded palettes. [confirmed from DESIGN.md §2]
- svelte-check baseline: 0 errors target; warning baseline drifts (currently 172 warnings + 1 pre-existing error in WechatHoverButton.svelte).
- Performance: avoid unbounded canvas/WebGL instances; background canvases pause offscreen.
- Accessibility: keyboard operability, focus rings, prefers-reduced-motion respected.

## Brand Commitments

- App name: "ST 控制台"; brand mark "ST"; version "v1.0 专业版".
- Brand accent: cyan `#22d3ee` ("青蓝品牌主色") is a stated brand commitment; it may shift shade for contrast but the cyan identity is binding. [confirmed from DESIGN.md]
- Voice/language: Simplified Chinese UI.

## Evidence on Hand

- Real app, real data flows; screenshots under E:\ST\.codex_shots; DESIGN.md is the incumbent visual-system record (to be replaced after this redesign).
- No marketing claims, testimonials, or external evidence exist; do not fabricate.

## Product Principles

[inferred from code + DESIGN.md; to be confirmed]
1. Operate first: scanning, consistency, and real data outrank decoration.
2. One visual vocabulary across every panel; module variety only in brand accents.
3. Real backend state is always visible and never faked.
4. Personalization tokens stay live: every surface derives from `--app-*`.
5. Local-first and private data stay on this machine.

## Accessibility & Inclusion

[inferred; not user-confirmed] Keyboard-first (shortcuts, focus rings), reduced-motion support, Chinese UI text. No other product-specific accessibility requirement established.
