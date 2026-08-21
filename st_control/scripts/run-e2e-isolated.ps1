# ============================================================
# run-e2e-isolated.ps1 - isolated E2E environment launcher
#
# Purpose: run E2E probes against an ISOLATED data directory so the
# real data/ is never touched. The app resolves its base dir from
# ST_WECHAT_APP_DIR first (common.rs app_base_dir); pointing it at
# .e2e/app gives a fully independent data/ (control.db, harness/,
# llm_config.json).
#
# Usage:
#   powershell -File scripts/run-e2e-isolated.ps1 -Probes "phase1,phase4"
#   powershell -File scripts/run-e2e-isolated.ps1 -KeepData
#
# Note: this script KILLS running st-control instances and vite dev
# servers (they may be the user's real app) - run it only in a test
# context. All messages are ASCII to avoid PS 5.1 encoding issues.
# ============================================================
param(
  [string]$Probes = "phase1",
  [switch]$KeepData
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$appBase = Join-Path $root ".e2e\app"
$exe = Join-Path $root "src-tauri\target\debug\st-control.exe"
$dataDir = Join-Path $appBase "data"

Write-Host "[e2e] root: $root"
Write-Host "[e2e] isolated app-base: $appBase"

# 0) prepare environment
New-Item -ItemType Directory -Path (Join-Path $dataDir "harness") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $dataDir "logs") -Force | Out-Null
if (-not $KeepData) {
  foreach ($f in @("control.db", "control.db-wal", "control.db-shm")) {
    Remove-Item (Join-Path $dataDir $f) -Force -ErrorAction SilentlyContinue
  }
  Remove-Item (Join-Path $dataDir "harness") -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Path (Join-Path $dataDir "harness") -Force | Out-Null
  Write-Host "[e2e] isolated data reset (clean state)"
}
if (-not (Test-Path (Join-Path $dataDir "llm_config.json"))) {
  Copy-Item (Join-Path $root "data\llm_config.json") (Join-Path $dataDir "llm_config.json") -Force
  Write-Host "[e2e] llm_config.json seeded"
}
# isolated app-base mimics some project-root traits (self-maintain probes read package.json)
if (-not (Test-Path (Join-Path $appBase "package.json"))) {
  Set-Content -Path (Join-Path $appBase "package.json") -Value '{ "name": "e2e-isolated-app", "private": true }' -Encoding Ascii
  Write-Host "[e2e] package.json seeded (isolated workspace root)"
}
if (-not (Test-Path $exe)) { throw "exe not found: $exe (run cargo build first)" }

# 1) stop existing instances (test context only)
function Stop-TestInstances {
  # teardown core: kill app + vite (idempotent; failures are silent)
  Get-Process -Name "st-control" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  # Stop-Process is an async signal: poll until the app really exits
  # (a leftover process locks the exe and blocks the next cargo build)
  for ($i = 0; $i -lt 10; $i++) {
    if (-not (Get-Process -Name "st-control" -ErrorAction SilentlyContinue)) { break }
    Start-Sleep -Milliseconds 300
  }
  try {
    Get-CimInstance Win32_Process -Filter "Name='node.exe'" -ErrorAction Stop |
      Where-Object { $_.CommandLine -match "vite|ensure-port-1420" } |
      ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  } catch {
    # CIM query occasionally hangs / lacks permission: fall back to killing
    # by process name (may hit the user's vite; acceptable in test context)
    Write-Host "[e2e] CIM query failed, fallback kill by name: $($_.Exception.Message)"
    Get-Process -Name "node" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  }
}
Stop-TestInstances
Start-Sleep -Seconds 1

# 2) start vite (background)
Write-Host "[e2e] starting vite :1420 ..."
Start-Process -FilePath "npm.cmd" -ArgumentList "run", "dev" -WorkingDirectory $root -WindowStyle Hidden | Out-Null
$viteOk = $false
for ($i = 0; $i -lt 60; $i++) {
  try { if ((Invoke-WebRequest -Uri "http://localhost:1420" -TimeoutSec 2 -UseBasicParsing).StatusCode -eq 200) { $viteOk = $true; break } } catch {}
  Start-Sleep -Seconds 1
}
if (-not $viteOk) { throw "vite not ready" }
Write-Host "[e2e] vite ready"

# 3) start isolated app instance (CDP 9222 + ST_WECHAT_APP_DIR)
Write-Host "[e2e] starting isolated st-control.exe ..."
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
$env:ST_WECHAT_APP_DIR = $appBase
Start-Process -FilePath $exe -WorkingDirectory $appBase | Out-Null
$cdpOk = $false
for ($i = 0; $i -lt 60; $i++) {
  try { if ((Invoke-WebRequest -Uri "http://127.0.0.1:9222/json/list" -TimeoutSec 2 -UseBasicParsing).StatusCode -eq 200) { $cdpOk = $true; break } } catch {}
  Start-Sleep -Seconds 1
}
if (-not $cdpOk) { throw "CDP not ready" }
Write-Host "[e2e] CDP :9222 ready"
# wait for the page to load and expose the Tauri IPC bridge
# (first vite compile of ~4700 modules can take 20-60s)
$bridgeOk = $false
for ($i = 0; $i -lt 90; $i++) {
  & node ".e2e\_wait-ipc.mjs" 2>$null
  if ($LASTEXITCODE -eq 0) { $bridgeOk = $true; break }
  Start-Sleep -Seconds 2
}
if (-not $bridgeOk) { throw "Tauri IPC bridge not ready (page still loading)" }
Write-Host "[e2e] page IPC bridge ready"

# 4) run probes
$failed = @()
# 4.0) static gate: IPC contract audit before any probe (fast, zero-LLM;
#      catches ack/resync key-name drift before spending LLM calls)
Push-Location $root
& node ".codex_tests\smoke-ipc-contract.mjs"
$ipcCode = $LASTEXITCODE
Pop-Location
if ($ipcCode -ne 0) {
  Write-Host "[e2e] IPC contract audit FAILED (exit $ipcCode); aborting probes"
  $failed += "ipc-contract"
}
try {
  foreach ($p in ($Probes -split ",")) {
    $p = $p.Trim()
    if (-not $p) { continue }
    # IPC contract gate failed: skip probes (do not spend LLM calls on a
    # known-broken build); teardown still runs via finally
    if ($failed -contains "ipc-contract") { break }
    Write-Host "`n[e2e] ==== probe: $p ===="
    Push-Location $root
    # probe name resolution: e2e-harness-<name>.mjs by default;
    # names starting with "verify-" map directly (verify-harness-*.mjs etc.)
    $script = if ($p -like "verify-*") { ".codex_tests\$p.mjs" } else { ".codex_tests\e2e-harness-$p.mjs" }
    & node $script
    $code = $LASTEXITCODE
    Pop-Location
    if ($code -ne 0) { $failed += $p; Write-Host "[e2e] $p FAILED (exit $code)" }
    else { Write-Host "[e2e] $p ALL_PASS" }
  }
} catch {
  # probe-run error (script throw / node crash) also counts as failure
  # and must still reach teardown
  Write-Host "[e2e] probe run error: $($_.Exception.Message)"
  $failed += "run-error"
} finally {
  # 5) teardown: ALWAYS stop the isolated app + vite so the next run
  #    starts clean and cargo build is not blocked by a locked exe
  #    (only stop the instances this script started: st-control + vite node)
  Stop-TestInstances
  Write-Host "[e2e] teardown done"
}

Write-Host "`n[e2e] done."
if ($failed.Count -gt 0) { Write-Host "[e2e] failed probes: $($failed -join ', ')" }
else { Write-Host "[e2e] all passed (real data/ untouched)" }

if ($failed.Count -gt 0) { exit 1 }
