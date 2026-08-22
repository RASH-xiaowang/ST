Set-Location "C:\Users\28361\Desktop\ST"
Remove-Item -Force "cleanup3.ps1" -ErrorAction SilentlyContinue

Write-Host "=== Key Project Files ==="
$files = @(
    "AGENTS.md",
    "README.md",
    ".gitignore",
    ".github\workflows\ci.yml",
    "st_control\package.json",
    "st_control\vite.config.ts",
    "st_control\config.json",
    "st_control\src-tauri\Cargo.toml",
    "st_control\src-tauri\src\main.rs",
    "st_agent\package.json",
    "st_web\package.json"
)
foreach ($f in $files) {
    $exists = Test-Path $f
    if ($exists) { Write-Host "  [OK] $f" }
    else { Write-Host "  [MISSING] $f" }
}

Write-Host "
=== Source Code ==="
$srcDirs = @("st_control\src", "st_control\src-tauri\src", "st_agent\src", "st_web\src")
foreach ($d in $srcDirs) {
    if (Test-Path $d) {
        $count = (Get-ChildItem -Path $d -Recurse -File).Count
        Write-Host "  $d : $count files"
    }
}
