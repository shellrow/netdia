<#
.SYNOPSIS
  Sign Tauri Windows installers (.exe/.msi) with a smart-card backed certificate.
  Optionally auto-detect latest installers under src-tauri\target\release\bundle\ and
  save signed copies with standard names.

.DESCRIPTION
  - If -InputExe/-OutputExe and/or -InputMsi/-OutputMsi are provided, sign those paths.
  - If nothing is provided, auto-detect the latest NSIS .exe and MSI under:
      src-tauri\target\release\bundle\nsis\NetDia_*_x64-setup.exe
      src-tauri\target\release\bundle\msi\NetDia_*_x64_*.msi
    and copy them to the signed/ dir (or -OutputDir) as:
      netdia-vX.Y.Z-windows-x64-setup-signed.exe
      netdia-vX.Y.Z-windows-x64_<locale>-signed.msi
    where X.Y.Z and locale are extracted from the detected file names.

  - After signing, verifies signatures and writes SHA256 hashes to artifacts-sha256.txt.

.NOTES
  - Requires signtool.exe (Windows SDK / VS Build Tools) and a connected smart card.
  - You will be prompted for PIN during signing.
#>

[CmdletBinding()]
param(
  [string]$SignToolPath = "",
  [string]$TimestampUrl = "http://timestamp.digicert.com",

  # Explicit inputs/outputs (use any combination)
  [string]$InputExe = "",
  [string]$OutputExe = "",
  [string]$InputMsi = "",
  [string]$OutputMsi = "",

  # When auto-detecting, where to put signed copies (default: signed/)
  [string]$OutputDir = ""
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

    # Heuristics for a Tauri repo root:
    # - src-tauri directory exists
    # - OR Cargo.toml exists (workspace/repo root)
    if (Test-Path (Join-Path $candidate "src-tauri")) { return $candidate }
    if (Test-Path (Join-Path $candidate "Cargo.toml")) { return $candidate }

    $parent = Split-Path -Parent $candidate
    if (-not $parent -or $parent -eq $candidate) {
      throw "Could not resolve repo root from: $startDir (src-tauri/Cargo.toml not found in parents)"
    }
    $dir = Resolve-Path $parent
  }
}

function Resolve-SignTool([string]$explicitPath) {
  if ($explicitPath -and (Test-Path $explicitPath)) {
    return (Resolve-Path $explicitPath).Path
  }

  $cmd = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }

  $kitsRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
  if (Test-Path $kitsRoot) {
    $found = Get-ChildItem $kitsRoot -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
      Sort-Object FullName -Descending |
      Select-Object -First 1
    if ($found) { return $found.FullName }
  }

  throw "signtool.exe was not found. Install Windows SDK (or VS Build Tools) or pass -SignToolPath."
}

function Escape-Arg([string]$s) {
  if ($null -eq $s) { return '""' }
  # Quote if needed, and escape embedded quotes
  if ($s -match '[\s"]') {
    return '"' + ($s -replace '"','\"') + '"'
  }
  return $s
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

  # Build a safe command line string
  $argLine = ($argv | ForEach-Object { Escape-Arg $_ }) -join " "
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

function Ensure-Dir([string]$dir) {
  if (!(Test-Path $dir)) {
    New-Item -ItemType Directory -Path $dir | Out-Null
  }
}

function Copy-For-Signing([string]$src, [string]$dst) {
  Ensure-Dir (Split-Path $dst -Parent)
  Copy-Item -Force $src $dst
}

function Sign-File([string]$signtool, [string]$filePath, [string]$tsUrl) {
  Write-Host "Signing: $filePath" -ForegroundColor Green
  Exec $signtool @("sign","/a","/fd","SHA256","/td","SHA256","/tr",$tsUrl,$filePath) (Split-Path $filePath -Parent)
}

function Verify-File([string]$signtool, [string]$filePath) {
  Write-Host "Verifying: $filePath" -ForegroundColor Green
  Exec $signtool @("verify","/pa","/v",$filePath) (Split-Path $filePath -Parent)
}

function Sha256([string]$filePath) {
  (Get-FileHash -Algorithm SHA256 -Path $filePath).Hash.ToLowerInvariant()
}

function Parse-ExeName([string]$name) {
  # NetDia_0.4.0_x64-setup.exe
  $m = [regex]::Match($name, '^NetDia_(?<ver>\d+\.\d+\.\d+)_x64-setup\.exe$', 'IgnoreCase')
  if (!$m.Success) { return $null }
  return @{
    Version = $m.Groups["ver"].Value
  }
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

function Detect-Latest([string]$repoRoot) {
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

  return @{
    Exe = $exe
    Msi = $msi
  }
}

# --- Main ---
Write-Section "Resolve paths"
#$RepoRoot = (Resolve-Path $RepoRoot).Path
$ScriptPath = $MyInvocation.MyCommand.Path
if (-not $ScriptPath) {
    throw "Cannot determine script path. Please run this file as a script (powershell -File ...)."
}

$ScriptDir = Split-Path -Parent $ScriptPath
$RepoRoot = Resolve-RepoRoot $ScriptDir

Write-Host "RepoRoot: $RepoRoot"

$signtool = Resolve-SignTool $SignToolPath
Write-Host "signtool: $signtool"

if (!$OutputDir) {
  $OutputDir = Join-Path $RepoRoot "signed"
}
Ensure-Dir $OutputDir
$OutputDir = (Resolve-Path $OutputDir).Path
Write-Host "OutputDir: $OutputDir"

$explicitMode = ($InputExe -or $OutputExe -or $InputMsi -or $OutputMsi)
$workItems = @()

if ($explicitMode) {
  if ($InputExe -and !$OutputExe) { throw "InputExe was provided but OutputExe is empty." }
  if ($OutputExe -and !$InputExe) { throw "OutputExe was provided but InputExe is empty." }
  if ($InputMsi -and !$OutputMsi) { throw "InputMsi was provided but OutputMsi is empty." }
  if ($OutputMsi -and !$InputMsi) { throw "OutputMsi was provided but InputMsi is empty." }

  if ($InputExe) {
    if (!(Test-Path $InputExe)) { throw "InputExe not found: $InputExe" }
    $workItems += [pscustomobject]@{ Kind="exe"; Input=(Resolve-Path $InputExe).Path; Output=$OutputExe }
  }
  if ($InputMsi) {
    if (!(Test-Path $InputMsi)) { throw "InputMsi not found: $InputMsi" }
    $workItems += [pscustomobject]@{ Kind="msi"; Input=(Resolve-Path $InputMsi).Path; Output=$OutputMsi }
  }
}
else {
  Write-Section "Auto-detect latest NetDia installers"
  $det = Detect-Latest $RepoRoot

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

  if ($det.Exe) {
    $outExeName = "netdia-v$version-windows-x64-setup-signed.exe"
    $outExePath = Join-Path $OutputDir $outExeName
    $workItems += [pscustomobject]@{ Kind="exe"; Input=$det.Exe.FullName; Output=$outExePath }
  } else {
    Write-Host "No NSIS .exe found; skipping exe signing." -ForegroundColor Yellow
  }

  if ($det.Msi) {
    $outMsiName = "netdia-v$version-windows-x64_$locale-signed.msi"
    $outMsiPath = Join-Path $OutputDir $outMsiName
    $workItems += [pscustomobject]@{ Kind="msi"; Input=$det.Msi.FullName; Output=$outMsiPath }
  } else {
    Write-Host "No .msi found; skipping msi signing." -ForegroundColor Yellow
  }
}

if ($workItems.Count -eq 0) {
  throw "Nothing to do (no work items)."
}

Write-Section "Plan"
$workItems | ForEach-Object {
  Write-Host ("[{0}] {1} -> {2}" -f $_.Kind.ToUpper(), $_.Input, $_.Output)
}

Write-Section "Copy (create signed copies)"
foreach ($w in $workItems) {
  Copy-For-Signing $w.Input $w.Output
}

Write-Section "Sign"
foreach ($w in $workItems) {
  Sign-File $signtool $w.Output $TimestampUrl
}

Write-Section "Verify"
foreach ($w in $workItems) {
  Verify-File $signtool $w.Output
}

Write-Section "SHA256 hashes"
$hashLines = @()
foreach ($w in $workItems) {
  $h = Sha256 $w.Output
  $rel = $w.Output.Replace($RepoRoot + "\", "")
  $line = "$h  $rel"
  $hashLines += $line
  Write-Host $line
}

$hashOut = Join-Path $OutputDir "artifacts-sha256.txt"
$hashLines | Set-Content -Encoding UTF8 -Path $hashOut
Write-Host ""
Write-Host "Wrote: $hashOut" -ForegroundColor Cyan

Write-Host ""
Write-Host "All Done!" -ForegroundColor Green
