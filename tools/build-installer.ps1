# Builds Mochi.exe and mochi-cli.exe in release mode and compiles the Inno Setup installer into dist/.
# Usage: ./tools/build-installer.ps1 [-Version 0.1.0] [-SkipBuild] [-PublishOnly] [-SkipPublish]
param(
    [string]$Version = "0.1.0",
    [switch]$SkipBuild,
    [switch]$PublishOnly,
    [switch]$SkipPublish
)
$ErrorActionPreference = "Stop"

$root = Split-Path $PSScriptRoot -Parent
$publish = Join-Path $root "publish"
$dist = Join-Path $root "dist"

if (-not $env:MOCHI_GOOGLE_CLIENT_ID -or -not $env:MOCHI_GOOGLE_CLIENT_SECRET) {
    $dataDir = if ($env:MOCHI_DATA_DIR) { $env:MOCHI_DATA_DIR } else { Join-Path $env:LOCALAPPDATA "Mochi" }
    $candidates = @(
        (Join-Path $root "google-client.json"),
        (Join-Path $dataDir "google-client.json")
    )
    if (Test-Path $dataDir) {
        $candidates += (Get-ChildItem $dataDir -Filter "client_secret*.json" -ErrorAction SilentlyContinue | Sort-Object Name | Select-Object -ExpandProperty FullName)
    }
    foreach ($cand in $candidates) {
        if ($cand -and (Test-Path $cand)) {
            try {
                $doc = Get-Content -LiteralPath $cand -Raw | ConvertFrom-Json
                $node = if ($doc.installed) { $doc.installed } elseif ($doc.web) { $doc.web } else { $doc }
                if ($node.client_id -and $node.client_secret) {
                    $env:MOCHI_GOOGLE_CLIENT_ID = [string]$node.client_id
                    $env:MOCHI_GOOGLE_CLIENT_SECRET = [string]$node.client_secret
                    Write-Host "Loaded Google OAuth client from $(Split-Path $cand -Leaf)"
                    break
                }
            } catch {
            }
        }
    }
}

if (-not $SkipPublish) {
    if (-not $SkipBuild) {
        Push-Location $root
        try {
            cargo build --release --workspace
            if ($LASTEXITCODE -ne 0) { throw "cargo build --release --workspace failed" }
        } finally {
            Pop-Location
        }
    }

    New-Item -ItemType Directory -Force -Path $publish | Out-Null
    Copy-Item -Force (Join-Path $root "target\release\Mochi.exe") (Join-Path $publish "Mochi.exe")
    Copy-Item -Force (Join-Path $root "target\release\mochi-cli.exe") (Join-Path $publish "mochi-cli.exe")
    if ($PublishOnly) { return }
}

$iscc = @(
    (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1

if (-not $iscc) {
    throw "Inno Setup 6 not found. Install it from https://jrsoftware.org/isdl.php or via winget install JRSoftware.InnoSetup"
}

New-Item -ItemType Directory -Force -Path $dist | Out-Null
& $iscc /Q "/DAppVersion=$Version" "/DPublishDir=$publish" (Join-Path $root "installer\mochi.iss")
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }

$versioned = Join-Path $dist "Mochi-Setup-$Version.exe"
$latest = Join-Path $dist "Mochi-Setup.exe"
Copy-Item -Force $versioned $latest

$sums = @(
    $versioned,
    $latest,
    (Join-Path $publish "Mochi.exe"),
    (Join-Path $publish "mochi-cli.exe")
) | Where-Object { Test-Path $_ } | ForEach-Object {
    $h = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
    "$h  $(Split-Path $_ -Leaf)"
}
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllLines((Join-Path $dist "SHA256SUMS.txt"), $sums, $utf8NoBom)

Get-ChildItem $dist | Select-Object Name, Length
