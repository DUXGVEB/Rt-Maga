<#
.SYNOPSIS
    Downloads and verifies RtMaga Tier 1 (Lite) PP-OCRv6 tiny ONNX assets.
.DESCRIPTION
    Fetches official PaddlePaddle PP-OCRv6_tiny detection, recognition, and dictionary
    files into gitignored desktop (`src-tauri/resources/ai-lite/`) and Android
    (`src-tauri/gen/android/app/src/main/assets/ai-lite/`) staging folders, verifying
    exact byte lengths, SHA-256 digests, and the < 35 MB Tier 1 budget.
#>
[CmdletBinding()]
param(
    [switch]$SkipAndroid,
    [switch]$SkipDesktop
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
$DesktopDir = Join-Path $RepoRoot 'src-tauri\resources\ai-lite'
$AndroidDir = Join-Path $RepoRoot 'src-tauri\gen\android\app\src\main\assets\ai-lite'
$BudgetBytes = 35 * 1024 * 1024

$Assets = @(
    @{
        Name   = 'det.onnx'
        Url    = 'https://huggingface.co/PaddlePaddle/PP-OCRv6_tiny_det_onnx/resolve/main/inference.onnx'
        Bytes  = 1780590L
        Sha256 = '193bab7a04fca699a6c82e6abb5b81bdb28177f0abd4062552b04908dafb19f8'
    },
    @{
        Name   = 'rec.onnx'
        Url    = 'https://huggingface.co/PaddlePaddle/PP-OCRv6_tiny_rec_onnx/resolve/main/inference.onnx'
        Bytes  = 4462639L
        Sha256 = '9ef676d6ed3c88256a2d92c640c44f25b0c40947e111b14b8be8f594091563e6'
    },
    @{
        Name   = 'inference.yml'
        Url    = 'https://huggingface.co/PaddlePaddle/PP-OCRv6_tiny_rec_onnx/resolve/main/inference.yml'
        Bytes  = 55571L
        Sha256 = '66170210bad538e83fff3c4a3867e547d6bf20b50d64b20347c4b913f3034ea1'
    }
)

function Test-VerifiedFile {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][long]$ExpectedBytes,
        [Parameter(Mandatory)][string]$ExpectedSha256
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }
    $len = (Get-Item -LiteralPath $Path).Length
    if ($len -ne $ExpectedBytes) {
        return $false
    }
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    return ($actual -eq $ExpectedSha256.ToLowerInvariant())
}

$Targets = @()
if (-not $SkipDesktop) { $Targets += $DesktopDir }
if (-not $SkipAndroid) { $Targets += $AndroidDir }
if ($Targets.Count -eq 0) {
    throw 'Both -SkipDesktop and -SkipAndroid were set; nothing to fetch.'
}

foreach ($dir in $Targets) {
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
}

$PrimaryDir = $Targets[0]
[long]$TotalBytes = 0

foreach ($asset in $Assets) {
    $dest = Join-Path $PrimaryDir $asset.Name
    if (Test-VerifiedFile -Path $dest -ExpectedBytes $asset.Bytes -ExpectedSha256 $asset.Sha256) {
        Write-Host "[OK] Cached & verified $($asset.Name) ($($asset.Bytes) B)"
    } else {
        $part = "$dest.part"
        if (Test-Path -LiteralPath $part) {
            Remove-Item -LiteralPath $part -Force
        }
        try {
            Write-Host "[DL] Downloading $($asset.Name) from $($asset.Url)..."
            Invoke-WebRequest -Uri $asset.Url -OutFile $part -UseBasicParsing
            $actualLen = (Get-Item -LiteralPath $part).Length
            if ($actualLen -ne $asset.Bytes) {
                throw "Size mismatch for $($asset.Name): expected $($asset.Bytes) B, got $actualLen B"
            }
            $actualSha = (Get-FileHash -LiteralPath $part -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actualSha -ne $asset.Sha256.ToLowerInvariant()) {
                throw "SHA-256 mismatch for $($asset.Name): expected $($asset.Sha256), got $actualSha"
            }
            Move-Item -LiteralPath $part -Destination $dest -Force
            Write-Host "[OK] Verified $($asset.Name) ($actualLen B, sha256=$actualSha)"
        } catch {
            if (Test-Path -LiteralPath $part) {
                Remove-Item -LiteralPath $part -Force
            }
            throw
        }
    }

    for ($i = 1; $i -lt $Targets.Count; $i++) {
        $mirrorDest = Join-Path $Targets[$i] $asset.Name
        if (-not (Test-VerifiedFile -Path $mirrorDest -ExpectedBytes $asset.Bytes -ExpectedSha256 $asset.Sha256)) {
            Copy-Item -LiteralPath $dest -Destination $mirrorDest -Force
        }
    }

    $TotalBytes += $asset.Bytes
}

if ($TotalBytes -ge $BudgetBytes) {
    throw "Tier 1 model footprint ($TotalBytes B) exceeded strict 35 MB budget ($BudgetBytes B)"
}

$TotalMb = [Math]::Round($TotalBytes / 1MB, 2)
Write-Host "[PASS] Tier 1 AI models ready ($TotalMb MB < 35 MB budget)."
