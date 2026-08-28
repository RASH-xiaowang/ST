<#
  安装本地 Git 钩子（DSH quality-gates：低成本缺陷本地发现）。
  用法：powershell -File scripts/install-git-hooks.ps1
  将本仓库的 core.hooksPath 指向 .githooks/（pre-commit 快检 cargo fmt）。
#>
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
git config core.hooksPath .githooks
Write-Host '已安装：git config core.hooksPath = .githooks（pre-commit 快检 cargo fmt --check）'
