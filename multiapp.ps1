# multiapp.ps1 — prototype multi-profile launcher for Windows
# Companion to the macOS/Linux `multiapp` bash script. Same verified mechanism:
#   Start-Process <App.exe> -ArgumentList "--user-data-dir=<profile dir>"
#     (EQUALS form; the isolation lever is Electron/Chromium, so it is cross-platform)
# No binaries are modified. No env overrides needed. Windows Credential store is never touched.
#
# STATUS: implemented following the verified macOS mechanism + multigravity.ps1 conventions,
#         but NOT yet tested on real Windows hardware. Treat verdicts as provisional until
#         you run `multiapp probe <app>` on your machine.
#
# Install:  copy this file somewhere on PATH as multiapp.cmd shim, or call directly:
#           powershell -ExecutionPolicy Bypass -File multiapp.ps1 <command> ...

param([Parameter(Position=0)][string]$Command = "help",
      [Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)

$ErrorActionPreference = "Stop"
$VERSION = "0.2.0"

$Root      = if ($env:MULTIAPP_HOME) { $env:MULTIAPP_HOME } else { Join-Path $env:APPDATA "Multiapp" }
$Profiles  = Join-Path $Root "Profiles"
$Trash     = Join-Path $Root "Trash"
$UserReg   = Join-Path $Root "registry.local"
$Wrappers  = [Environment]::GetFolderPath("Programs")   # Start Menu\Programs
$ClaudeMain= Join-Path $env:APPDATA "Claude"            # Claude desktop userData on Windows

# scan locations for Electron apps
$ScanDirs = @(
  (Join-Path $env:LOCALAPPDATA "Programs"),
  $env:ProgramFiles,
  ${env:ProgramFiles(x86)}
) | Where-Object { $_ -and (Test-Path $_) }

# ---- built-in registry (Windows paths are best-effort; run `scan` to be sure) ----
# key ; exe path ; display ; adapter ; verdict ; note
$Builtin = @"
claude;$env:LOCALAPPDATA\AnthropicClaude\claude.exe;Claude;electron;supported;
chatgpt;$env:LOCALAPPDATA\Programs\ChatGPT\ChatGPT.exe;ChatGPT;electron;partial;Data isolates but the ACCOUNT is shared across profiles. Do NOT sign out inside a profile.
notion;$env:LOCALAPPDATA\Programs\Notion\Notion.exe;Notion;electron;supported;
vscode;$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe;Visual Studio Code;vscode;supported;
github;$env:LOCALAPPDATA\GitHubDesktop\GitHubDesktop.exe;GitHub Desktop;electron;supported;
telegram;$env:APPDATA\Telegram Desktop\Telegram.exe;Telegram;none;unsupported;Telegram has native multi-account; profiles are unnecessary.
"@

function Get-Registry {
  $rows = @()
  foreach ($line in ($Builtin -split "`n")) {
    $line = $line.Trim(); if (-not $line) { continue }
    $p = $line -split ";", 6
    $rows += [pscustomobject]@{ key=$p[0]; path=$p[1]; name=$p[2]; adapter=$p[3]; verdict=$p[4]; note=$p[5] }
  }
  if (Test-Path $UserReg) {
    foreach ($line in (Get-Content $UserReg)) {
      $line = $line.Trim(); if (-not $line) { continue }
      $p = $line -split ";", 6
      $rows += [pscustomobject]@{ key=$p[0]; path=$p[1]; name=$p[2]; adapter=$p[3]; verdict=$p[4]; note=$p[5] }
    }
  }
  $rows
}

function Reg($key) { Get-Registry | Where-Object { $_.key -eq $key } | Select-Object -First 1 }
function Die($m)   { Write-Error "multiapp: $m"; exit 1 }
function Warn($m)  { Write-Host "!!  $m" -ForegroundColor Yellow }
function Info($m)  { Write-Host $m }
function Test-Name($n) { if ($n -notmatch '^[a-zA-Z0-9][a-zA-Z0-9-]{0,40}$') { Die "invalid profile name '$n'" } }
function ProfileDir($key,$name) { Join-Path (Join-Path $Profiles $key) $name }

function Require-App($key) {
  $r = Reg $key
  if (-not $r) { Die "unknown app '$key' — see: multiapp apps" }
  if ($r.verdict -eq "unsupported") { Die "'$key' is UNSUPPORTED: $($r.note)" }
  if ($r.verdict -eq "untested")    { Warn "'$key' is untested — verify: multiapp probe $key" }
  if (-not (Test-Path $r.path))     { Die "app not installed: $($r.path)" }
  $r
}

function Running-Procs($dir) {
  Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -and $_.CommandLine.Contains("--user-data-dir=$dir") }
}
function Is-Running($dir) { [bool](Running-Procs $dir) }

function Launch($exe, $data, $extra) {
  $args = @("--user-data-dir=$data") + $extra
  Start-Process -FilePath $exe -ArgumentList $args
}

# ------------------------------------------------------------------ commands
function Cmd-Apps {
  "{0,-20} {1,-22} {2,-11} {3}" -f "KEY","APP","VERDICT","NOTE"
  foreach ($r in Get-Registry) {
    $v = if (Test-Path $r.path) { $r.verdict } else { "not installed" }
    "{0,-20} {1,-22} {2,-11} {3}" -f $r.key,$r.name,$v,$r.note
  }
}

function Cmd-Scan {
  New-Item -ItemType Directory -Force -Path $Root | Out-Null
  Info "scanning for profile-capable apps (windows)…"
  $added=0; $known=0; $nonelectron=0
  foreach ($d in $ScanDirs) {
    Get-ChildItem -Path $d -Directory -ErrorAction SilentlyContinue | ForEach-Object {
      $exe = Get-ChildItem -Path $_.FullName -Filter *.exe -ErrorAction SilentlyContinue | Select-Object -First 1
      if (-not $exe) { return }
      # Electron heuristic: a resources\app.asar next to the exe, or Chromium data files
      $isElectron = (Test-Path (Join-Path $_.FullName "resources\app.asar")) -or (Test-Path (Join-Path $_.FullName "icudtl.dat"))
      if (-not $isElectron) { $nonelectron++; return }
      if (Get-Registry | Where-Object { $_.path -eq $exe.FullName }) { $known++; return }
      $base = $_.Name
      $key = ($base.ToLower() -replace '[^a-z0-9]+','-').Trim('-')
      if (-not $key) { return }
      if (Get-Registry | Where-Object { $_.key -eq $key }) { return }
      "$key;$($exe.FullName);$base;electron;untested;found by scan (windows); verify: multiapp probe $key" |
        Out-File -Append -Encoding utf8 $UserReg
      Info "  + $base  ->  key: $key  (electron, untested)"; $added++
    }
  }
  Info ""; Info "scan done: $added added, $known known, $nonelectron non-electron skipped"
  if ($added -gt 0) { Info "verify with:  multiapp probe <key>" }
}

function Cmd-New($key,$name) {
  if (-not $key -or -not $name) { Die "usage: multiapp new <app> <profile>" }
  Test-Name $name; Require-App $key | Out-Null
  $dir = ProfileDir $key $name
  if (Test-Path $dir) { Die "profile already exists: $dir" }
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $r = Reg $key; if ($r.note) { Warn $r.note }
  Info "created profile '$name' for $key"; Info "launch it:  multiapp launch $key $name"
}

function Cmd-Launch($key,$name,$passthru) {
  if (-not $key -or -not $name) { Die "usage: multiapp launch <app> <profile> [-- app args]" }
  Test-Name $name; $r = Require-App $key
  $dir = ProfileDir $key $name
  if (-not (Test-Path $dir)) { Die "no such profile '$name' for '$key' — create it: multiapp new $key $name" }
  $data = Join-Path $dir "data"
  if (Is-Running $data) { Warn "profile '$name' already has a running instance" }
  if ($r.note) { Warn $r.note }
  $extra = @()
  if ($r.adapter -eq "vscode") { $ext = Join-Path $dir "extensions"; New-Item -ItemType Directory -Force -Path $ext | Out-Null; $extra = @("--extensions-dir","$ext") }
  New-Item -ItemType Directory -Force -Path $data | Out-Null
  Launch $r.path $data ($extra + $passthru)
  Info "launched $key/$name"
}

function Cmd-List($filter) {
  if (-not (Test-Path $Profiles)) { Info "no profiles yet — create one: multiapp new <app> <profile>"; return }
  "{0,-16} {1,-20} {2,-9}" -f "APP","PROFILE","STATE"
  Get-ChildItem $Profiles -Directory | ForEach-Object {
    $key = $_.Name; if ($filter -and $key -ne $filter) { return }
    Get-ChildItem $_.FullName -Directory | ForEach-Object {
      $data = Join-Path $_.FullName "data"
      $state = if (Is-Running $data) { "running" } else { "stopped" }
      "{0,-16} {1,-20} {2,-9}" -f $key,$_.Name,$state
    }
  }
}

function Cmd-Stop($key,$name) {
  if (-not $key -or -not $name) { Die "usage: multiapp stop <app> <profile>" }
  $data = Join-Path (ProfileDir $key $name) "data"
  $procs = Running-Procs $data
  if (-not $procs) { Die "no running instance for $key/$name" }
  # stop the top-level process (shortest command line = main, not a --type= helper)
  $main = $procs | Where-Object { $_.CommandLine -notmatch '--type=' } | Select-Object -First 1
  if (-not $main) { $main = $procs | Select-Object -First 1 }
  Stop-Process -Id $main.ProcessId
  Info "sent quit to $key/$name (pid $($main.ProcessId))"
}

function Cmd-Clone($key,$src,$dst) {
  if (-not $key -or -not $src -or -not $dst) { Die "usage: multiapp clone <app> <src> <dst>" }
  Test-Name $src; Test-Name $dst; Require-App $key | Out-Null
  $s = ProfileDir $key $src; $d = ProfileDir $key $dst
  if (-not (Test-Path $s)) { Die "no such profile: $key/$src" }
  if (Test-Path $d) { Die "target already exists: $key/$dst" }
  if (Is-Running (Join-Path $s "data")) { Die "stop '$src' before cloning" }
  Warn "cloning copies session cookies — the server will see a duplicated session"
  Copy-Item -Recurse $s $d
  Info "cloned $key/$src -> $key/$dst"
}

function Cmd-Rename($key,$old,$new) {
  if (-not $key -or -not $old -or -not $new) { Die "usage: multiapp rename <app> <old> <new>" }
  Test-Name $old; Test-Name $new; Require-App $key | Out-Null
  $o = ProfileDir $key $old; $n = ProfileDir $key $new
  if (-not (Test-Path $o)) { Die "no such profile: $key/$old" }
  if (Test-Path $n) { Die "target name already exists: $key/$new" }
  if (Is-Running (Join-Path $o "data")) { Die "profile is running — stop it first" }
  Move-Item $o $n
  Info "renamed $key/$old -> $key/$new"
  $lnk = Join-Path $Wrappers ("$((Reg $key).name) - $old.lnk")
  if (Test-Path $lnk) { Remove-Item $lnk; Cmd-Wrapper $key $new }
}

function Cmd-Delete($key,$name) {
  if (-not $key -or -not $name) { Die "usage: multiapp delete <app> <profile>" }
  Test-Name $name
  $dir = ProfileDir $key $name
  if (-not (Test-Path $dir)) { Die "no such profile: $key/$name" }
  if (Is-Running (Join-Path $dir "data")) { Die "profile is running — stop it first" }
  $ans = Read-Host "Type the profile name to move '$key/$name' to Multiapp Trash (recoverable)"
  if ($ans -ne $name) { Die "aborted (name mismatch)" }
  New-Item -ItemType Directory -Force -Path $Trash | Out-Null
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  Move-Item $dir (Join-Path $Trash "${key}__${name}__$stamp")
  Info "moved to $Trash — restore by moving it back, purge with: multiapp trash purge"
}

function Cmd-Trash($sub) {
  switch ($sub) {
    "purge" {
      if (-not (Test-Path $Trash)) { Info "trash is empty"; return }
      if ((Read-Host "Permanently delete everything in $Trash ? [y/N]") -ne "y") { Die "aborted" }
      Remove-Item -Recurse -Force $Trash; Info "trash purged"
    }
    default { if (Test-Path $Trash) { Get-ChildItem $Trash } else { Info "trash is empty" } }
  }
}

function Cmd-Wrapper($key,$name) {
  if (-not $key -or -not $name) { Die "usage: multiapp wrapper <app> <profile>" }
  Test-Name $name; $r = Require-App $key
  $dir = ProfileDir $key $name
  if (-not (Test-Path $dir)) { Die "no such profile: $key/$name" }
  $self = $PSCommandPath
  $lnk = Join-Path $Wrappers ("$($r.name) - $name.lnk")
  $sh = New-Object -ComObject WScript.Shell
  $s = $sh.CreateShortcut($lnk)
  $s.TargetPath = "powershell.exe"
  $s.Arguments  = "-WindowStyle Hidden -ExecutionPolicy Bypass -File `"$self`" launch $key $name"
  $s.IconLocation = $r.path
  $s.Save()
  Info "shortcut created: $lnk  (pin it to Start/Taskbar)"
}

function Cmd-Probe($key) {
  if (-not $key) { Die "usage: multiapp probe <app>" }
  $r = Reg $key; if (-not $r) { Die "unknown app '$key'" }
  if (-not (Test-Path $r.path)) { Die "not installed: $($r.path)" }
  $tmp = Join-Path $Root "Probes\$key-$PID"
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  Info "probing $key (launches for ~10s)…"
  Launch $r.path $tmp @()
  Start-Sleep 10
  $honored = (Get-ChildItem $tmp -ErrorAction SilentlyContinue | Measure-Object).Count -gt 0
  Running-Procs $tmp | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep 2
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  $verdict = if ($honored) { "HONORED"; $nv="supported" } else { "IGNORED"; $nv="unsupported" }
  Info "$key: --user-data-dir= $verdict"
  if (Test-Path $UserReg) {
    $lines = Get-Content $UserReg | ForEach-Object {
      if ($_ -match "^$key;") { $p = $_ -split ";",6; $p[4]=$nv; $p[5]="probe $(Get-Date -Format yyyy-MM-dd)"; ($p -join ";") } else { $_ }
    }
    $lines | Set-Content $UserReg
    Info "registry updated: $key -> $nv"
  }
}

function Cmd-Doctor {
  Info "multiapp $VERSION  (windows)"
  Info "os:        $((Get-CimInstance Win32_OperatingSystem).Caption)"
  Info "root:      $Root"
  $n = 0; if (Test-Path $Profiles) { $n = (Get-ChildItem $Profiles -Directory -Recurse -Depth 1 -ErrorAction SilentlyContinue | Where-Object { $_.Parent.Parent.Name -eq "Profiles" }).Count }
  Info "profiles:  $n"
  foreach ($r in Get-Registry) { if (-not (Test-Path $r.path)) { Info "  missing app: $($r.key) ($($r.path))" } }
}

function Cmd-Help {
@"
multiapp $VERSION — run multiple isolated profiles of desktop apps (windows)

usage:  multiapp <command> [args]      (via: powershell -File multiapp.ps1 ...)

  apps                          list known apps + support verdicts
  scan                          discover installed Electron apps -> registry
  new     <app> <profile>       create a profile
  launch  <app> <profile>       launch an isolated instance
  list    [app]                 list profiles (state)
  stop    <app> <profile>       stop a profile's instance
  clone   <app> <src> <dst>     duplicate a stopped profile
  rename  <app> <old> <new>     rename a stopped profile (shortcut regenerated)
  delete  <app> <profile>       move a profile to Multiapp Trash (staged)
  trash   [list|purge]          inspect / empty the staged trash
  wrapper <app> <profile>       create a Start-Menu .lnk launcher
  probe   <app>                 canary-test whether the app honors the flag
  doctor                        environment self-check
  help                          this text

session transfer/export/import: use the macOS/Linux `multiapp` for now (Claude Code
  session model verified there; Windows port pending).

mechanism: Start-Process <App> -ArgumentList "--user-data-dir=<profile>"   (equals form)
profiles live in: $Profiles
"@
}

# ------------------------------------------------------------------ dispatch
$a = $Rest
switch ($Command) {
  "apps"    { Cmd-Apps }
  "scan"    { Cmd-Scan }
  "new"     { Cmd-New   $a[0] $a[1] }
  "launch"  { Cmd-Launch $a[0] $a[1] ($a | Select-Object -Skip 2) }
  "list"    { Cmd-List  $a[0] }
  "ls"      { Cmd-List  $a[0] }
  "stop"    { Cmd-Stop  $a[0] $a[1] }
  "clone"   { Cmd-Clone $a[0] $a[1] $a[2] }
  "rename"  { Cmd-Rename $a[0] $a[1] $a[2] }
  "delete"  { Cmd-Delete $a[0] $a[1] }
  "trash"   { Cmd-Trash $a[0] }
  "wrapper" { Cmd-Wrapper $a[0] $a[1] }
  "probe"   { Cmd-Probe $a[0] }
  "doctor"  { Cmd-Doctor }
  default   { Cmd-Help }
}
