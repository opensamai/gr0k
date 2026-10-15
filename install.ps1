# gr0k installer (Windows x86_64).
#
# Downloads the x86_64-pc-windows-msvc artifact from this repo's GitHub
# Releases, verifies its SHA-256 against the release's SHA256SUMS manifest,
# and installs the binary as %USERPROFILE%\.gr0k\bin\gr0k.exe.
#
# Usage:
#   irm https://raw.githubusercontent.com/opensamai/gr0k/dev/install.ps1 | iex
#   powershell -ExecutionPolicy Bypass -File install.ps1 -Version v0.2.109
#
# Environment:
#   GR0K_SHARE_DIR         install root (default: %USERPROFILE%\.gr0k; legacy HYPER_SHARE_DIR still honored)
#   GR0K_UPDATE_BASE_URL   GitHub-Releases-shaped API base (default:
#                          https://api.github.com/repos/opensamai/gr0k/releases)

[CmdletBinding()]
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Fail([string]$Message) {
    Write-Error "install.ps1: error: $Message"
    exit 1
}

$Repo = "opensamai/gr0k"
$ApiBase = if ($env:GR0K_UPDATE_BASE_URL) { $env:GR0K_UPDATE_BASE_URL } elseif ($env:HYPER_UPDATE_BASE_URL) { $env:HYPER_UPDATE_BASE_URL } else { "https://api.github.com/repos/$Repo/releases" }
$Gr0kHome = if ($env:GR0K_SHARE_DIR) { $env:GR0K_SHARE_DIR } elseif ($env:HYPER_SHARE_DIR) { $env:HYPER_SHARE_DIR } else { Join-Path $env:USERPROFILE ".gr0k" }
$Triple = "x86_64-pc-windows-msvc"

# ── Platform gate ────────────────────────────────────────────────────────────
if (-not [System.Environment]::Is64BitOperatingSystem) {
    Fail "gr0k requires 64-bit Windows (x86_64)"
}
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64") {
    Fail "unsupported architecture '$arch' (only x86_64/AMD64 Windows builds are published)"
}

# ── Version argument ─────────────────────────────────────────────────────────
$Version = $Version.TrimStart("v")
if ($Version -and $Version -notmatch '^\d+\.\d+\.\d+([-.][0-9A-Za-z.-]+)?$') {
    Fail "invalid version '$Version' (expected X.Y.Z or vX.Y.Z)"
}

# TLS 1.2 for older PowerShell 5.1 defaults.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$Headers = @{ "User-Agent" = "gr0k-install"; "Accept" = "application/vnd.github+json" }

# ── Resolve the release ──────────────────────────────────────────────────────
$ReleaseUrl = if ($Version) { "$ApiBase/tags/v$Version" } else { "$ApiBase/latest" }
Write-Host "Resolving release from $ReleaseUrl"
try {
    $Release = Invoke-RestMethod -Uri $ReleaseUrl -Headers $Headers
} catch {
    Fail "could not fetch release metadata from ${ReleaseUrl}: $($_.Exception.Message)"
}

$Tag = [string]$Release.tag_name
if (-not $Tag) { Fail "release metadata has no tag_name (endpoint: $ReleaseUrl)" }
$ResolvedVersion = $Tag.TrimStart("v")
if ($Version -and $ResolvedVersion -ne $Version) {
    Fail "requested version $Version but release tag is $Tag"
}

$Asset = "gr0k-$ResolvedVersion-$Triple.zip"
$ArchiveAsset = $Release.assets | Where-Object { $_.name -eq $Asset } | Select-Object -First 1
$SumsAsset = $Release.assets | Where-Object { $_.name -eq "SHA256SUMS" } | Select-Object -First 1
if (-not $ArchiveAsset) { Fail "release $Tag has no asset $Asset" }
if (-not $SumsAsset) { Fail "release $Tag has no SHA256SUMS asset; refusing to install unverified binaries" }

# ── Download + verify ────────────────────────────────────────────────────────
$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("gr0k-install-" + [System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null
try {
    $ArchivePath = Join-Path $TmpDir $Asset
    $SumsPath = Join-Path $TmpDir "SHA256SUMS"

    Write-Host "Downloading gr0k v$ResolvedVersion ($Triple)..."
    Invoke-WebRequest -Uri $ArchiveAsset.browser_download_url -Headers $Headers -OutFile $ArchivePath
    Invoke-WebRequest -Uri $SumsAsset.browser_download_url -Headers $Headers -OutFile $SumsPath

    $Expected = $null
    foreach ($line in Get-Content $SumsPath) {
        $parts = $line.Trim() -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $Asset) {
            $Expected = $parts[0].ToLowerInvariant()
        }
    }
    if (-not $Expected) { Fail "SHA256SUMS has no entry for $Asset" }

    $Actual = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) {
        Fail "SHA256 mismatch for ${Asset}: expected $Expected, got $Actual"
    }
    Write-Host "Checksum verified."

    # ── Extract + install ────────────────────────────────────────────────────
    $ExtractDir = Join-Path $TmpDir "extracted"
    Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir -Force
    $Binary = Get-ChildItem -Path $ExtractDir -Recurse -Filter "gr0k.exe" | Select-Object -First 1
    if (-not $Binary) { Fail "archive $Asset does not contain gr0k.exe" }

    $BinDir = Join-Path $Gr0kHome "bin"
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    $Dest = Join-Path $BinDir "gr0k.exe"

    # Smoke-test the downloaded binary *before* replacing the active install.
    & $Binary.FullName --version *> $null
    if ($LASTEXITCODE -ne 0) {
        Fail "downloaded binary failed smoke test (exit $LASTEXITCODE); existing install left untouched"
    }

    # A running gr0k.exe blocks writes but allows renames — move it aside first.
    $Aside = "$Dest.old"
    if (Test-Path $Dest) {
        Remove-Item -Path $Aside -Force -ErrorAction SilentlyContinue
        try {
            Move-Item -Path $Dest -Destination $Aside -Force
        } catch {
            Fail "cannot replace $Dest (close all running gr0k sessions and retry): $($_.Exception.Message)"
        }
    }
    try {
        Move-Item -Path $Binary.FullName -Destination $Dest -Force
    } catch {
        if (Test-Path $Aside) {
            Move-Item -Path $Aside -Destination $Dest -Force -ErrorAction SilentlyContinue
        }
        Fail "cannot install to $Dest: $($_.Exception.Message)"
    }

    # Secondary smoke-test of the activated path; restore prior binary on failure.
    & $Dest --version *> $null
    if ($LASTEXITCODE -ne 0) {
        if (Test-Path $Aside) {
            Remove-Item -Path $Dest -Force -ErrorAction SilentlyContinue
            Move-Item -Path $Aside -Destination $Dest -Force -ErrorAction SilentlyContinue
        }
        Fail "installed binary failed to run (exit $LASTEXITCODE); previous install restored if available"
    }

    Write-Host ""
    Write-Host "gr0k v$ResolvedVersion installed to $Dest"

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $OnPath = (($UserPath -split ";") -contains $BinDir) -or
              (($env:Path -split ";") -contains $BinDir)
    if (-not $OnPath) {
        $NewUserPath = if ($UserPath) { "$BinDir;$UserPath" } else { $BinDir }
        [Environment]::SetEnvironmentVariable("Path", $NewUserPath, "User")
        Write-Host ""
        Write-Host "Added $BinDir to your user PATH."
        Write-Host "Open a new terminal, then run 'gr0k' to get started."
    } else {
        Write-Host "Run 'gr0k' to get started."
    }
} finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
