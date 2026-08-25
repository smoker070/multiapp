# Multiapp — Windows verification run.
#
# Everything in this repository was written on a Mac. This script is what turns "Windows is
# designed for" into "Windows is tested", and it is deliberately noisy about which half failed.
#
#   powershell -ExecutionPolicy Bypass -File .\scripts\test-windows.ps1
#
# It builds nothing outside this folder, installs nothing without asking, and the profiles it
# creates live in a temporary directory that it removes afterwards.

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$rust = Join-Path $repo 'rust'

function Section($t) { Write-Host ""; Write-Host "== $t" -ForegroundColor Cyan }
function Ok($t)      { Write-Host "   PASS  $t" -ForegroundColor Green }
function Bad($t)     { Write-Host "   FAIL  $t" -ForegroundColor Red }

$results = [ordered]@{}

Section "environment"
Write-Host "   windows : $([System.Environment]::OSVersion.Version)"
Write-Host "   repo    : $repo"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "Rust is not installed. Install it, then re-run this script:" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "   winget install Rustlang.Rustup" -ForegroundColor White
    Write-Host ""
    Write-Host "(or download from https://rustup.rs). Close and reopen PowerShell afterwards so"
    Write-Host "cargo is on PATH. Nothing else is needed — no Visual Studio project, no SDK beyond"
    Write-Host "the MSVC build tools rustup offers to install for you."
    exit 1
}
Write-Host "   cargo   : $(cargo --version)"

# Which browser the integration test will drive. Edge ships with Windows, so this normally just works.
$edge = @(
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
    "${env:ProgramFiles}\Microsoft\Edge\Application\msedge.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($edge) { Write-Host "   edge    : $edge" }
else       { Write-Host "   edge    : NOT FOUND — the live test will skip, not fail" -ForegroundColor Yellow }

Push-Location $rust
try {
    Section "1/4  build"
    cargo build --release --bin multiapp
    if ($LASTEXITCODE -eq 0) { Ok "multiapp.exe built"; $results['build'] = $true }
    else                     { Bad "build failed";      $results['build'] = $false; return }

    Section "2/4  unit tests"
    cargo test --lib
    if ($LASTEXITCODE -eq 0) { Ok "unit tests"; $results['unit'] = $true }
    else                     { Bad "unit tests"; $results['unit'] = $false }

    Section "3/4  live app test (launches Edge, then stops it)"
    # This is the one that matters. It asserts the app wrote into an isolated profile, that a
    # prefix-named sibling profile is NOT reported as running, and that a graceful stop works.
    # Graceful stop on Windows is `taskkill` without /F, i.e. WM_CLOSE — this is the first time
    # that path has ever run.
    cargo test --test live_app -- --nocapture
    if ($LASTEXITCODE -eq 0) { Ok "live app test"; $results['live'] = $true }
    else                     { Bad "live app test"; $results['live'] = $false }

    Section "4/4  end-to-end CLI against a real app"
    $exe  = Join-Path $rust 'target\release\multiapp.exe'
    $demo = Join-Path ([System.IO.Path]::GetTempPath()) ("multiapp-win-" + [guid]::NewGuid().ToString('N').Substring(0,8))
    $env:MULTIAPP_HOME = $demo
    try {
        & $exe --version
        & $exe new  "Microsoft Edge" work
        & $exe launch "Microsoft Edge" work
        Start-Sleep -Seconds 8
        Write-Host "   --- list (expect: work = running) ---"
        & $exe list
        $listed = (& $exe list | Out-String)
        & $exe stop "Microsoft Edge" work
        Start-Sleep -Seconds 2
        Write-Host "   --- list (expect: work = stopped) ---"
        & $exe list
        $after = (& $exe list | Out-String)

        $ranOk     = $listed -match 'running'
        $stoppedOk = $after  -notmatch 'running'
        if ($ranOk)     { Ok "profile reported as running while Edge was up" }
        else            { Bad "profile never reported running — check resolve_app / the flag" }
        if ($stoppedOk) { Ok "profile reported as stopped after stop" }
        else            { Bad "still running after stop — graceful quit needs an escalation policy" }
        $results['e2e'] = ($ranOk -and $stoppedOk)
    } finally {
        Remove-Item env:MULTIAPP_HOME -ErrorAction SilentlyContinue
        if (Test-Path $demo) { Remove-Item -Recurse -Force $demo -ErrorAction SilentlyContinue }
    }
} finally {
    Pop-Location
}

Section "summary"
foreach ($k in $results.Keys) {
    if ($results[$k]) { Ok $k } else { Bad $k }
}
if ($results.Values -contains $false) {
    Write-Host ""
    Write-Host "Something failed. Copy this whole output back — the failure IS the result we wanted." -ForegroundColor Yellow
    exit 1
}
Write-Host ""
Write-Host "All green. Windows is verified, not assumed." -ForegroundColor Green
