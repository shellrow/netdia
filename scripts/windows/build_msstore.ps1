<#
.SYNOPSIS
  Build Tauri with Microsoft Store config and collect installers into build/ with standard names.

.DESCRIPTION
  Runs:
    cargo tauri build --config src-tauri/tauri.microsoftstore.conf.json

  Then finds the latest installers under:
    src-tauri\target\release\bundle\nsis\NetDia_*_x64-setup.exe
    src-tauri\target\release\bundle\msi\NetDia_*_x64_*.msi

  And copies them to:
    <repoRoot>\build\
  as:
    netdia-vX.Y.Z-windows-x64-setup.exe
    netdia-vX.Y.Z-windows-x64_<locale>.msi

.NOTES
  - Assumes the generated files are named like:
      NetDia_0.4.0_x64-setup.exe
      NetDia_0.4.0_x64_en-US.msi
  - No signing is performed in this script.
#>

[CmdletBinding()]
param(
  [string]$ConfigPath = "src-tauri/tauri.microsoftstore.conf.json",
  [string]$OutputDirName = "build"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Section([string]$title) {
  Write-Host ""
  Write-Host "== $title ==" -ForegroundColor Cyan
}

function Resolve-RepoRoot([string]$startDir) {
  $dir = Resolve-Path $startDir
  while ($true) {
    $candidate = $dir.Path
    if (Test-Path (Join-Path $candidate "src-tauri")) { return $candidate }
    if (Test-Path (Join-Path $candidate "Cargo.toml")) { return $candidate }
    $parent = Split-Path -Parent $candidate
    if (-not $parent -or $parent -eq $candidate) {
      throw "Could not resolve repo root from: $startDir (src-tauri/Cargo.toml not found in parents)"
    }
    $dir = Resolve-Path $parent
  }
}

function Ensure-Dir([string]$dir) {
  if (!(Test-Path $dir)) {
    New-Item -ItemType Directory -Path $dir | Out-Null
  }
}

function Exec([string]$file, [string[]]$argv, [string]$workingDir) {
  if (-not $argv -or $argv.Count -eq 0) {
    throw "Exec(): argv is empty for file: $file"
  }

  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $file
  $psi.WorkingDirectory = $workingDir
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError  = $true
  $psi.UseShellExecute = $false

  # PS5.1-friendly arguments string
  $argLine = ($argv | ForEach-Object {
    if ($_ -match '[\s"]') { '"' + ($_ -replace '"','\"') + '"' } else { $_ }
  }) -join " "
  $psi.Arguments = $argLine

  Write-Host ">> $file $argLine" -ForegroundColor DarkGray

  $p = New-Object System.Diagnostics.Process
  $p.StartInfo = $psi
  [void]$p.Start()
  $stdout = $p.StandardOutput.ReadToEnd()
  $stderr = $p.StandardError.ReadToEnd()
  $p.WaitForExit()

  if ($stdout) { Write-Host $stdout.TrimEnd() }
  if ($stderr) { Write-Host $stderr.TrimEnd() -ForegroundColor Yellow }

  if ($p.ExitCode -ne 0) {
    throw "Command failed with exit code $($p.ExitCode): $file $argLine"
  }
}

function Parse-ExeName([string]$name) {
  # NetDia_0.4.0_x64-setup.exe
  $m = [regex]::Match($name, '^NetDia_(?<ver>\d+\.\d+\.\d+)_x64-setup\.exe$', 'IgnoreCase')
  if (!$m.Success) { return $null }
  return @{ Version = $m.Groups["ver"].Value }
}

function Parse-MsiName([string]$name) {
  # NetDia_0.4.0_x64_en-US.msi
  $m = [regex]::Match($name, '^NetDia_(?<ver>\d+\.\d+\.\d+)_x64_(?<loc>[A-Za-z]{2}-[A-Za-z]{2})\.msi$', 'IgnoreCase')
  if (!$m.Success) { return $null }
  return @{
    Version = $m.Groups["ver"].Value
    Locale  = $m.Groups["loc"].Value
  }
}

function Detect-Latest-Installers([string]$repoRoot) {
  $bundleRoot = Join-Path $repoRoot "src-tauri\target\release\bundle"
  $nsisDir = Join-Path $bundleRoot "nsis"
  $msiDir  = Join-Path $bundleRoot "msi"

  if (!(Test-Path $nsisDir) -and !(Test-Path $msiDir)) {
    throw "Could not find bundle directories. Expected under: $bundleRoot"
  }

  $exe = $null
  if (Test-Path $nsisDir) {
    $exe = Get-ChildItem $nsisDir -Recurse -File -Filter "*.exe" -ErrorAction SilentlyContinue |
      Where-Object { Parse-ExeName $_.Name } |
      Sort-Object LastWriteTime -Descending |
      Select-Object -First 1
  }

  $msi = $null
  if (Test-Path $msiDir) {
    $msi = Get-ChildItem $msiDir -Recurse -File -Filter "*.msi" -ErrorAction SilentlyContinue |
      Where-Object { Parse-MsiName $_.Name } |
      Sort-Object LastWriteTime -Descending |
      Select-Object -First 1
  }

  if (!$exe -and !$msi) {
    throw "No matching NetDia installers found under: $bundleRoot"
  }

  return @{ Exe = $exe; Msi = $msi }
}

# --- Main ---
Write-Section "Resolve paths"
$ScriptPath = $MyInvocation.MyCommand.Path
if (-not $ScriptPath) { throw "Cannot determine script path. Run via: powershell -File <path>" }
$ScriptDir = Split-Path -Parent $ScriptPath
$RepoRoot = Resolve-RepoRoot $ScriptDir
Write-Host "RepoRoot: $RepoRoot"

$ConfigFullPath = Join-Path $RepoRoot $ConfigPath
if (!(Test-Path $ConfigFullPath)) {
  throw "Config not found: $ConfigFullPath"
}
Write-Host "Config: $ConfigFullPath"

$OutDir = Join-Path $RepoRoot $OutputDirName
Ensure-Dir $OutDir
$OutDir = (Resolve-Path $OutDir).Path
Write-Host "OutputDir: $OutDir"

Write-Section "Build (Tauri Microsoft Store config)"
Exec "cargo" @("tauri","build","--config",$ConfigFullPath) $RepoRoot

Write-Section "Detect latest installers"
$det = Detect-Latest-Installers $RepoRoot

$version = $null
$locale  = "en-US"

if ($det.Msi) {
  $info = Parse-MsiName $det.Msi.Name
  $version = $info.Version
  $locale  = $info.Locale
} elseif ($det.Exe) {
  $info = Parse-ExeName $det.Exe.Name
  $version = $info.Version
} else {
  throw "Unexpected: neither MSI nor EXE detected."
}

Write-Host ("Detected version: {0}, locale: {1}" -f $version, $locale)

Write-Section "Copy to build/"
$copied = @()

if ($det.Exe) {
  $dstExe = Join-Path $OutDir ("netdia-v{0}-windows-x64-setup.exe" -f $version)
  Copy-Item -Force $det.Exe.FullName $dstExe
  $copied += $dstExe
  Write-Host "EXE: $dstExe" -ForegroundColor Green
} else {
  Write-Host "No NSIS .exe found; skipping." -ForegroundColor Yellow
}

if ($det.Msi) {
  $dstMsi = Join-Path $OutDir ("netdia-v{0}-windows-x64_{1}.msi" -f $version, $locale)
  Copy-Item -Force $det.Msi.FullName $dstMsi
  $copied += $dstMsi
  Write-Host "MSI: $dstMsi" -ForegroundColor Green
} else {
  Write-Host "No .msi found; skipping." -ForegroundColor Yellow
}

Write-Section "SHA256"
$hashOut = Join-Path $OutDir "artifacts-sha256.txt"
$lines = @()
foreach ($p in $copied) {
  $h = (Get-FileHash -Algorithm SHA256 -Path $p).Hash.ToLowerInvariant()
  $rel = $p.Replace($RepoRoot + "\", "")
  $line = "$h  $rel"
  $lines += $line
  Write-Host $line
}
$lines | Set-Content -Encoding UTF8 -Path $hashOut
Write-Host "Wrote: $hashOut" -ForegroundColor Cyan

Write-Host ""
Write-Host "All Done!" -ForegroundColor Green
