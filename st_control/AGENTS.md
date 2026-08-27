# Repository Guidelines

## Project Structure & Module Organization

`st_control` is a Windows desktop app (Svelte 5 + TypeScript + Vite on Rust/Tauri 2); `st_agent` auto-connects to the ST Control server.

- `src/lib/<feature>/` — frontend code grouped by feature, with `components/`, `services/`, `utils/` subfolders.
- `src-tauri/src/<feature>/` — Rust backend, one module per feature; shared infra in root files (`db.rs`, `ws_server.rs`).
- `src-tauri/tests/` — Rust integration tests; `scripts/` — dev tooling; `.codex_tests/` — smoke/regression scripts.

## Build, Test, and Development Commands

From `st_control/`:

- `npm run dev` — Vite dev server on port 1420; `npm run tauri dev` — full desktop app.
- `npm run build` — production frontend build; `npm run tauri build` — packaged app.
- `npx svelte-check --output human` — type-check (0 errors, 0 warnings); `npm run audit:ui` — UI audit; `npm test` — Vitest.

From `st_control/src-tauri/`:

- `cargo test --lib --no-default-features` — Rust unit tests matching CI (`scripts/run-rust-tests.ps1`).
- `cargo fmt --check` and `cargo clippy --lib --no-default-features` — must pass with 0 warnings.

> `target/debug/st-control.exe` is a dev build needing `npm run dev` (Vite:1420); standalone builds require the `custom-protocol` feature (`npm run tauri build`).

## Coding Style & Naming Conventions

- TypeScript: strict mode, Svelte 5 runes (`$state`, `$derived`, `$props`), no unused locals.
- Rust: `cargo fmt`, `snake_case` identifiers, `CamelCase` types; Chinese comments explain *why* (e.g., the tokio pin in `Cargo.toml`).
- Name feature modules with the folder prefix (`kbApi`, `wc-*`); keep pure helpers in `utils/`.

## Testing Guidelines

- Backend: inline `#[cfg(test)] mod tests` per module; integration tests under `src-tauri/tests/`.
- Frontend: Vitest (`src/**/*.test.ts` / `*.spec.ts`) plus smoke scripts in `.codex_tests/` (`smoke-*.mjs`, `run-store-test.mjs`, `voice.test.mjs`), each with a header comment; the regression gate runs them all.
- E2E: CDP scripts (`e2e-*.mjs`) require the app + Vite running; follow setup comments.

## Commit & Pull Request Guidelines

- Conventional prefixes (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`) scoped by module, e.g. `fix(kb): correct chunk offset`.
- Reference issues in the body; describe the change, commands run, and results.
- For UI changes, include screenshots and note verified interactions.

## Security & Configuration

- Root `config.json` holds live secrets (WeChat DB keys, API tokens); never commit real values. Encrypt new persisted secrets (AES-256-CBC) as in `bot/secret.rs`.
- WeChat data is encrypted, local-only, and privacy-sensitive; handle it carefully.
- App-owned data lives under the app base dir (`common::app_base_dir()`); no absolute paths in `config.json`.
- Keep `data/` in `vite.config.ts`'s `server.watch.ignored` — runtime DB writes otherwise trigger an HMR reload storm.
