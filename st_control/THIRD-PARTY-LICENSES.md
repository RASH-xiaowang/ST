# Third-Party Licenses

This document lists the licenses of third-party dependencies used in ST Control.

## Rust Dependencies (Cargo)

| Crate | License | Description |
|---|---|---|
| tauri 2 | MIT OR Apache-2.0 | Desktop application framework |
| tokio 1.48 | MIT | Async runtime |
| axum 0.8 | MIT | HTTP framework |
| rusqlite 0.31 | MIT | SQLite bindings |
| reqwest 0.12 | MIT OR Apache-2.0 | HTTP client |
| serde 1 | MIT OR Apache-2.0 | Serialization framework |
| serde_json 1 | MIT OR Apache-2.0 | JSON support |
| chrono 0.4 | MIT OR Apache-2.0 | Date/time library |
| regex 1 | MIT OR Apache-2.0 | Regular expressions |
| uuid 1 | MIT OR Apache-2.0 | UUID generation |
| log 0.4 | MIT OR Apache-2.0 | Logging facade |
| env_logger 0.11 | MIT OR Apache-2.0 | Logging implementation |
| bcrypt 0.15 | MIT | Password hashing |
| aes 0.8 | MIT OR Apache-2.0 | AES encryption |
| sha2 0.10 | MIT OR Apache-2.0 | SHA-256 hashing |
| hmac 0.12 | MIT OR Apache-2.0 | HMAC authentication |
| pbkdf2 0.12 | MIT OR Apache-2.0 | Key derivation |
| hex 0.4 | MIT OR Apache-2.0 | Hex encoding |
| base64 0.22 | MIT OR Apache-2.0 | Base64 encoding |
| image 0.25 | MIT OR Apache-2.0 | Image processing |
| zip 2 | MIT | ZIP archive support |
| zstd 0.13 | MIT | Zstandard compression |
| quick-xml 0.36 | MIT | XML parsing |
| encoding_rs 0.8 | Apache-2.0 | Character encoding |
| notify 6 | CC0-1.0 OR MIT OR Apache-2.0 | File system monitoring |
| lru 0.12 | MIT | LRU cache |
| rand 0.8 | MIT OR Apache-2.0 | Random number generation |
| r2d2 0.8 | MIT OR Apache-2.0 | Connection pooling |
| sysinfo 0.33 | MIT | System information |
| futures-util 0.3 | MIT OR Apache-2.0 | Async utilities |
| tower-http 0.6 | MIT | HTTP middleware |
| tokio-stream 0.1 | MIT | Async streams |
| tokio-tungstenite 0.21 | MIT | WebSocket support |
| anydoc 0.1 | MIT | Document parsing (docx/pdf/epub → Markdown) |
| rapidocr-core 0.2 | Apache-2.0 | OCR engine (PP-OCRv6) |
| whisper-rs 0.16 | MIT | Speech-to-text (whisper.cpp) |
| silk-decoder-rs 0.1.0 | MIT | SILK audio decoder |
| windows 0.62 | MIT OR Apache-2.0 | Windows API bindings |
| qrcode 0.14 | MIT OR Apache-2.0 | QR code generation |
| winreg 0.52 | MIT | Windows registry access |
| jpeg-encoder 0.6 | MIT | JPEG encoding |

## npm Dependencies (Frontend)

| Package | License | Description |
|---|---|---|
| svelte 5 | MIT | UI framework |
| vite 6 | MIT | Build tool |
| tailwindcss 4 | MIT | CSS framework |
| @tauri-apps/api 2 | MIT OR Apache-2.0 | Tauri frontend API |
| @lucide/svelte | ISC | Icon library |
| bits-ui 2 | MIT | UI component primitives |
| paneforge 1 | MIT | Panel layout |
| vaul-svelte 0.3 | MIT | Drawer component |
| sonner 2 | MIT | Toast notifications |
| d3-force 3 | ISC | Force-directed layout |
| layerchart 2 | MIT | Chart components |
| @tanstack/table-core 9 | MIT | Table utilities |
| mode-watcher 1 | MIT | Theme management |
| clsx 2 | MIT | Class name utility |
| tailwind-merge 3 | MIT | Tailwind class merging |
| tailwind-variants 3 | MIT | Tailwind variants |
| embla-carousel-svelte 8 | MIT | Carousel component |
| @internationalized/date 3 | Apache-2.0 | Date utilities |
| phosphor-svelte 3 | MIT | Icon library |
| fancy-ui-svelte 0.9 | MIT | UI effects |
| svelte-sonner 1 | MIT | Sonner for Svelte |

## Build Tools

| Tool | License | Description |
|---|---|---|
| @tauri-apps/cli 2 | MIT OR Apache-2.0 | Tauri CLI |
| @sveltejs/vite-plugin-svelte 5 | MIT | Svelte Vite plugin |
| svelte-check 4 | MIT | Svelte type checker |
| typescript 5 | Apache-2.0 | TypeScript compiler |
| vitest 4 | MIT | Test framework |
| playwright-core 1 | Apache-2.0 | E2E testing |
| @tailwindcss/vite 4 | MIT | Tailwind Vite plugin |
| tw-animate-css 1 | MIT | Tailwind animations |

## Notes

- All Rust crates are from [crates.io](https://crates.io/) and follow their stated licenses
- All npm packages are from [npmjs.com](https://www.npmjs.com/) and follow their stated licenses
- The project itself is licensed under MIT
- For commercial use, please review the terms of any external APIs (LLM providers, etc.) that the application integrates with
