# Repository Guidelines

## Overview

Two Windows desktop apps built with Svelte 5 + TypeScript + Vite on Rust/Tauri 2:

- `st_control` — the main control console (WeChat, knowledge base, LLM, agents, automation, OCR); CI runs against it.
- `st_agent` — a lightweight client that auto-connects to the ST Control server.

## Project Structure

- `st_control/src/lib/<feature>/` — frontend code grouped by feature, with `components/`, `services/`, `utils/` subfolders.
- `st_control/src-tauri/src/<feature>/` — Rust backend, one module per feature; shared infra lives in root files (`db.rs`, `ws_server.rs`).
- `st_control/scripts/` — dev/audit tooling; `st_control/.codex_tests/` — regression and smoke test scripts; root `.github/workflows/ci.yml` — CI.

## Development Commands

From `st_control/`:

- `npm run dev` — Vite dev server on port 1420; `npm run tauri dev` — full desktop app.
- `npm run build` — production frontend build; `npm run tauri build` packages the app.
- `npx svelte-check --output human` — Svelte/TypeScript type-check (must be 0 errors and 0 warnings); `npm run audit:ui` — UI audit.

From `st_control/src-tauri/`:

- `cargo test --lib --no-default-features` — Rust unit tests, matching CI (also runnable via `scripts/run-rust-tests.ps1`).
- `cargo build` — backend with default features (onnx-ocr, local-stt).

> 运行注意事项：`src-tauri/target/debug/st-control.exe` 是 dev 构建，必须配合
> `npm run dev`（Vite:1420）才能加载界面；直接双击会显示连接错误页。
> 独立运行的 exe 必须启用 `custom-protocol` feature：用 `npm run tauri build`
>（CLI 自动注入），或 `cargo build --release --features custom-protocol`。
> 不带该 feature 的 `cargo build --release` 仍是 dev 模式、加载 devUrl，
> 独立运行时界面白屏、导航与布局异常。
- `cargo fmt --check` — formatting check (must pass); `cargo clippy --lib --no-default-features` — lints (must be 0 warnings).

## Coding Style

- TypeScript: strict mode, Svelte 5 runes (`$state`, `$derived`, `$props`), no unused locals.
- Rust: `cargo fmt`, `snake_case` identifiers, `CamelCase` types, Chinese comments explaining *why* (e.g. the tokio pin in `Cargo.toml`).
- Name feature modules with the folder prefix (`kbApi`, `wc-*` tokens); keep pure helpers in `utils/`.

## Testing Guidelines

- Backend: inline `#[cfg(test)] mod tests` in each Rust module.
- Frontend: Node smoke/unit scripts under `st_control/.codex_tests/` (`run-store-test.mjs`, `smoke-format-utils.mjs`, `smoke-image-queue.mjs`, `smoke-moment-media.mjs`, `smoke-moment-video.mjs`, `smoke-db-utils.mjs`, `smoke-kb-graph-layout.mjs`, `smoke-kb-graph-style.mjs`, `smoke-chat-context.mjs`, `smoke-format-bytes.mjs`, `smoke-hook.mjs`, `smoke-kb-graph-utils.mjs`, `smoke-attachments.mjs`, `smoke-col-widths.mjs`, `smoke-wechat-misc.mjs`, `smoke-virtual-list.mjs`, `smoke-wechat-display.mjs`, `smoke-wechat-security.mjs`, `smoke-role-utils.mjs`, `smoke-kb-chat-utils.mjs`, `smoke-automation-display.mjs`, `smoke-kb-file-utils.mjs`, `smoke-system-format.mjs`, `smoke-search-text.mjs`, `smoke-wechat-session.mjs`, `smoke-ocr-display.mjs`, `smoke-panel-utils.mjs`, `smoke-color-utils.mjs`, `smoke-message-render.mjs`, `smoke-model-kind.mjs`, `smoke-cost-format.mjs`, `smoke-chart-geometry.mjs`, `smoke-async-utils.mjs`, `smoke-agent-form.mjs`, `smoke-annual-summary.mjs`, `smoke-chart-paths.mjs`, `smoke-wechat-records.mjs`, `smoke-daily-summary.mjs`, `smoke-dir-tree.mjs`, `smoke-filter-utils.mjs`, `smoke-graph-stats.mjs`, `smoke-realtime-msg.mjs`, `smoke-session-order.mjs`, `smoke-wiki-markdown.mjs`, `smoke-bot-steps.mjs`, `smoke-chart-spec.mjs`, `smoke-ipc-contract.mjs`, `voice.test.mjs`) with a header comment per script. The standard regression gate runs all `smoke-*.mjs` plus `run-store-test.mjs` and `voice.test.mjs`.
- E2E: CDP scripts (`e2e-*.mjs`) require the app and Vite running; follow the setup comments.
- Keep `svelte-check` at 0 errors and 0 warnings; keep `cargo clippy` at 0 warnings.

## Commit & Pull Request Guidelines

- Use conventional commit prefixes (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`) and scope them by module, e.g. `fix(kb): correct chunk offset`.
- Reference issues in the body; describe the change, commands run, and results.
- For UI changes, include screenshots and note verified interactions.

## Security & Configuration

- Root `config.json` holds live secrets (WeChat DB encryption keys, API tokens); never commit real values.
- WeChat data is encrypted, local-only, and privacy-sensitive; handle it carefully.
- Encrypt new persisted secrets (AES-256-CBC), mirroring `bot/secret.rs`.

## Data & Path Layout (J-15 unified scheme)

- Everything the app owns lives under the app base dir (deployment = install dir; dev = project root, resolved by `common::app_base_dir()` — never CWD): `config.json` at the base, all data under `data/`, WeChat data under `data/wechat/` (was `%APPDATA%\st_result`), roles under `data/roles/`, logs at `data/logs/app.log`.
- `config.json` must not contain absolute paths: `db_dir`/`keys_file`/`decrypted_dir`/`decoded_image_dir`/`wechat_root` are optional — empty = auto-detect (most-active WeChat account) or default under `data/wechat/`; relative values resolve against the app base dir. Do not reintroduce hardcoded paths (frontend included).
- Startup runs `common::migrate_legacy_dirs()` (idempotent) to pull in pre-J-15 `%APPDATA%\st-control|st_result|st_role` data and renames them to `*.legacy-backup`.
- If legacy `%APPDATA%\st_result` still exists, WeChat config defaults temporarily fall back to it (migration safety net).
- `vite.config.ts` must keep `data/` in `server.watch.ignored`: runtime DB writes under the project root otherwise trigger an HMR reload storm that hangs the dev server (app stuck on the splash screen). Any new runtime output dir under the project root must be added there too.
