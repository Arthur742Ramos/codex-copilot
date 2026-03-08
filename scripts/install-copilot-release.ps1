param(
    [Parameter(Position=0)]
    [string]$Version = "latest"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$Repo = if ([string]::IsNullOrWhiteSpace($env:CODEX_RELEASE_REPO)) {
    "Arthur742Ramos/codex-copilot"
} else {
    $env:CODEX_RELEASE_REPO
}

function Write-Step {
    param(
        [string]$Message
    )

    Write-Host "==> $Message"
}

function Normalize-Version {
    param(
        [string]$RawVersion
    )

    if ([string]::IsNullOrWhiteSpace($RawVersion) -or $RawVersion -eq "latest") {
        return "latest"
    }

    if ($RawVersion.StartsWith("copilot-v")) {
        return $RawVersion
    }

    if ($RawVersion.StartsWith("v")) {
        return "copilot-$RawVersion"
    }

    return "copilot-v$RawVersion"
}

function Resolve-Version {
    $normalizedVersion = Normalize-Version -RawVersion $Version
    if ($normalizedVersion -ne "latest") {
        return $normalizedVersion
    }

    $headers = @{
        "User-Agent" = "codex-copilot-installer"
        "Accept" = "application/vnd.github+json"
    }
    $release = Invoke-RestMethod -Headers $headers -Uri "https://api.github.com/repos/$Repo/releases/latest"
    if (-not $release.tag_name) {
        Write-Error "Failed to resolve the latest codex-copilot release version from $Repo."
        exit 1
    }

    return $release.tag_name
}

function Get-ReleaseUrl {
    param(
        [string]$AssetName,
        [string]$ResolvedVersion
    )

    return "https://github.com/$Repo/releases/download/$ResolvedVersion/$AssetName"
}

function Path-Contains {
    param(
        [string]$PathValue,
        [string]$Entry
    )

    if ([string]::IsNullOrWhiteSpace($PathValue)) {
        return $false
    }

    $needle = $Entry.TrimEnd("\")
    foreach ($segment in $PathValue.Split(";", [System.StringSplitOptions]::RemoveEmptyEntries)) {
        if ($segment.TrimEnd("\") -ieq $needle) {
            return $true
        }
    }

    return $false
}

if ($env:OS -ne "Windows_NT") {
    Write-Error "install-copilot-release.ps1 supports Windows only. Use install-copilot-release.sh on macOS or Linux."
    exit 1
}

if (-not [Environment]::Is64BitOperatingSystem) {
    Write-Error "codex-copilot requires a 64-bit version of Windows."
    exit 1
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
switch ($architecture) {
    "X64" {
        $target = "x86_64-pc-windows-msvc"
        $platformLabel = "Windows (x64)"
    }
    default {
        Write-Error "Unsupported Windows architecture: $architecture. The fork release workflow currently publishes x64 Windows assets."
        exit 1
    }
}

if ([string]::IsNullOrWhiteSpace($env:CODEX_INSTALL_DIR)) {
    $installDir = Join-Path $env:LOCALAPPDATA "Programs\codex-copilot\bin"
} else {
    $installDir = $env:CODEX_INSTALL_DIR
}

$codexPath = Join-Path $installDir "codex.exe"
$installMode = if (Test-Path $codexPath) { "Updating" } else { "Installing" }

Write-Step "$installMode codex-copilot"
Write-Step "Repo: $Repo"
Write-Step "Detected platform: $platformLabel"

$resolvedVersion = Resolve-Version
$assetName = "codex-$target.zip"
$downloadUrl = Get-ReleaseUrl -AssetName $assetName -ResolvedVersion $resolvedVersion

Write-Step "Release: $resolvedVersion"
Write-Step "Target: $target"

$headers = @{
    "User-Agent" = "codex-copilot-installer"
    "Accept" = "application/octet-stream"
}
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("codex-copilot-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

try {
    $archivePath = Join-Path $tempDir $assetName
    $extractDir = Join-Path $tempDir "extract"

    Write-Step "Downloading $assetName"
    Invoke-WebRequest -Headers $headers -Uri $downloadUrl -OutFile $archivePath

    New-Item -ItemType Directory -Force -Path $extractDir | Out-Null
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    Write-Step "Installing to $installDir"
    New-Item -ItemType Directory -Force -Path $installDir | Out-Null
    Copy-Item -Force (Join-Path $extractDir "codex.exe") $codexPath
} finally {
    Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
}

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$pathNeedsNewShell = $false
if (-not (Path-Contains -PathValue $userPath -Entry $installDir)) {
    if ([string]::IsNullOrWhiteSpace($userPath)) {
        $newUserPath = $installDir
    } else {
        $newUserPath = "$installDir;$userPath"
    }

    [Environment]::SetEnvironmentVariable("Path", $newUserPath, "User")
    if (-not (Path-Contains -PathValue $env:Path -Entry $installDir)) {
        if ([string]::IsNullOrWhiteSpace($env:Path)) {
            $env:Path = $installDir
        } else {
            $env:Path = "$installDir;$env:Path"
        }
    }
    Write-Step "PATH updated for future PowerShell sessions."
    $pathNeedsNewShell = $true
} elseif (Path-Contains -PathValue $env:Path -Entry $installDir) {
    Write-Step "$installDir is already on PATH."
} else {
    Write-Step "PATH is already configured for future PowerShell sessions."
    $pathNeedsNewShell = $true
}

if ($pathNeedsNewShell) {
    Write-Step ('Run now: $env:Path = "{0};$env:Path"; codex' -f $installDir)
    Write-Step "Or open a new PowerShell window and run: codex"
} else {
    Write-Step "Run: codex"
}

Write-Host "codex-copilot $resolvedVersion installed successfully."
