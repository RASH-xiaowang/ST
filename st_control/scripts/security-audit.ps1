# ============================================================
# 安全审计脚本（Rust + npm 依赖漏洞扫描）
#
# 用途：在发布前检查依赖库是否存在已知安全漏洞
# 用法：powershell -File scripts/security-audit.ps1
#
# 依赖：
#  - cargo-audit：cargo install cargo-audit
#  - npm：随 Node.js 安装
# ============================================================

$ErrorActionPreference = 'Continue'
$projectRoot = Join-Path $PSScriptRoot '..'
$hasIssues = $false

Write-Host '====================================' -ForegroundColor Cyan
Write-Host ' ST Control 安全审计' -ForegroundColor Cyan
Write-Host '====================================' -ForegroundColor Cyan
Write-Host ''

# ─── Rust 依赖审计 ───
Write-Host '[1/3] Rust 依赖漏洞扫描 (cargo audit)...' -ForegroundColor Yellow
Set-Location (Join-Path $projectRoot 'src-tauri')
try {
    $cargoAudit = cargo audit 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host '  ✅ 未发现已知漏洞' -ForegroundColor Green
    } else {
        Write-Host '  ⚠️ 发现潜在漏洞：' -ForegroundColor Red
        $cargoAudit | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
        $hasIssues = $true
    }
} catch {
    Write-Host '  ⚠️ cargo-audit 未安装，请运行: cargo install cargo-audit' -ForegroundColor Yellow
}

Write-Host ''

# ─── npm 依赖审计 ───
Write-Host '[2/3] npm 依赖漏洞扫描...' -ForegroundColor Yellow
Set-Location $projectRoot
try {
    $npmAudit = npm audit --json 2>&1 | Out-String
    $auditData = $npmAudit | ConvertFrom-Json -ErrorAction SilentlyContinue
    if ($auditData -and $auditData.metadata.vulnerabilities.total -eq 0) {
        Write-Host '  ✅ 未发现已知漏洞' -ForegroundColor Green
    } elseif ($auditData) {
        $vulns = $auditData.metadata.vulnerabilities
        Write-Host "  ⚠️ 发现漏洞: critical=$($vulns.critical) high=$($vulns.high) moderate=$($vulns.moderate) low=$($vulns.low)" -ForegroundColor Red
        $hasIssues = $true
    } else {
        Write-Host '  ✅ npm audit 完成（无漏洞或审计数据不可用）' -ForegroundColor Green
    }
} catch {
    Write-Host "  ⚠️ npm audit 执行失败: $_" -ForegroundColor Yellow
}

Write-Host ''

# ─── 许可证检查 ───
Write-Host '[3/3] 许可证文件检查...' -ForegroundColor Yellow
Set-Location $projectRoot

$licenseFile = Join-Path $projectRoot 'LICENSE'
$thirdPartyFile = Join-Path $projectRoot 'THIRD-PARTY-LICENSES.md'

if (Test-Path $licenseFile) {
    Write-Host '  ✅ LICENSE 文件存在' -ForegroundColor Green
} else {
    Write-Host '  ❌ LICENSE 文件缺失' -ForegroundColor Red
    $hasIssues = $true
}

if (Test-Path $thirdPartyFile) {
    Write-Host '  ✅ THIRD-PARTY-LICENSES.md 文件存在' -ForegroundColor Green
} else {
    Write-Host '  ⚠️ THIRD-PARTY-LICENSES.md 文件缺失' -ForegroundColor Yellow
}

Write-Host ''
Write-Host '====================================' -ForegroundColor Cyan
if ($hasIssues) {
    Write-Host ' 审计完成：发现问题，请查看上方详情' -ForegroundColor Red
    exit 1
} else {
    Write-Host ' 审计完成：未发现安全问题 ✅' -ForegroundColor Green
    exit 0
}
