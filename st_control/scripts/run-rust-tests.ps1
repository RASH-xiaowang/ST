<#
  Rust 单测运行脚本（Windows）

  背景：`cargo test --lib` 曾因测试二进制缺少 Common Controls v6 清单而
  在加载器阶段报 0xC0000139（rfd 静态导入 comctl32!TaskDialogIndirect），
  已由 build.rs 对测试目标嵌入清单解决（见 src-tauri/build.rs）。

  本脚本：用 `--no-default-features` 跑最小依赖面的库单测（与 CI 一致），
  不链接 ort/DirectML。
#>
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\src-tauri')

Write-Host '==> cargo test --lib --no-default-features' -ForegroundColor Cyan
cargo test --lib --no-default-features
exit $LASTEXITCODE
