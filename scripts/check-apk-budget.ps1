<#
.SYNOPSIS
    Verifies RtMaga Android APK size and AI asset budget compliance.
.DESCRIPTION
    Asserts:
      1. Total APK size <= 100 MB (104,857,600 bytes)
      2. Zero *.gguf files bundled anywhere inside the APK
      3. Combined Tier 1 assets (`assets/ai-lite/*`) + native AI libs
         (`libonnxruntime*.so`, `libllama*.so`, `libmtmd*.so`) < 35 MB
#>
[CmdletBinding()]
param(
    [string]$ApkPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.IO.Compression.FileSystem

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')
if ([string]::IsNullOrWhiteSpace($ApkPath)) {
    $candidates = @(
        (Join-Path $RepoRoot 'src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk'),
        (Join-Path $RepoRoot 'src-tauri\gen\android\app\build\outputs\apk\arm64\release\app-arm64-release.apk'),
        (Join-Path $RepoRoot 'src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk')
    )
    $found = $candidates | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    if (-not $found) {
        $discovered = Get-ChildItem -Path (Join-Path $RepoRoot 'src-tauri\gen\android\app\build\outputs\apk') -Filter '*.apk' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($discovered) {
            $found = $discovered.FullName
        }
    }
    if (-not $found) {
        throw 'No APK found. Pass -ApkPath <path-to-apk>.'
    }
    $ApkPath = $found
}

if (-not (Test-Path -LiteralPath $ApkPath -PathType Leaf)) {
    throw "APK not found at '$ApkPath'."
}

$MaxApkBytes = 100L * 1024L * 1024L
$MaxAiFootprintBytes = 35L * 1024L * 1024L

$ApkSize = (Get-Item -LiteralPath $ApkPath).Length
$ApkMb = [Math]::Round($ApkSize / 1MB, 2)

$GgufEntries = New-Object System.Collections.Generic.List[string]
[long]$Tier1AssetCompressed = 0
[long]$Tier1AssetUncompressed = 0
[long]$NativeAiCompressed = 0
[long]$NativeAiUncompressed = 0

$Zip = [System.IO.Compression.ZipFile]::OpenRead($ApkPath)
try {
    foreach ($entry in $Zip.Entries) {
        $name = $entry.FullName.Replace('\', '/')
        $lower = $name.ToLowerInvariant()

        if ($lower.EndsWith('.gguf')) {
            $GgufEntries.Add($name)
        }

        if ($lower.StartsWith('assets/ai-lite/')) {
            $Tier1AssetCompressed += $entry.CompressedLength
            $Tier1AssetUncompressed += $entry.Length
        }

        if ($lower -match '^lib/[^/]+/(libonnxruntime.*\.so|libllama.*\.so|libmtmd.*\.so)$') {
            $NativeAiCompressed += $entry.CompressedLength
            $NativeAiUncompressed += $entry.Length
        }
    }
} finally {
    $Zip.Dispose()
}

$AiCompressedTotal = $Tier1AssetCompressed + $NativeAiCompressed
$AiUncompressedTotal = $Tier1AssetUncompressed + $NativeAiUncompressed

Write-Host "APK: $ApkPath"
Write-Host ("Total APK size     : {0:N0} B ({1} MB / 100 MB limit)" -f $ApkSize, $ApkMb)
Write-Host ("Tier 1 assets      : {0:N0} B compressed / {1:N0} B raw" -f $Tier1AssetCompressed, $Tier1AssetUncompressed)
Write-Host ("Native AI .so libs : {0:N0} B compressed / {1:N0} B raw" -f $NativeAiCompressed, $NativeAiUncompressed)
Write-Host ("Combined AI budget : {0:N0} B compressed ({1} MB) / {2:N0} B raw ({3} MB) (< 35 MB limit)" -f `
    $AiCompressedTotal, [Math]::Round($AiCompressedTotal / 1MB, 2), `
    $AiUncompressedTotal, [Math]::Round($AiUncompressedTotal / 1MB, 2))

$Failures = New-Object System.Collections.Generic.List[string]
if ($ApkSize -gt $MaxApkBytes) {
    $Failures.Add("APK size ($ApkSize B) exceeds 100 MB limit ($MaxApkBytes B).")
}
if ($GgufEntries.Count -gt 0) {
    $Failures.Add("Forbidden .gguf model weights bundled in APK: $($GgufEntries -join ', ').")
}
if ($AiCompressedTotal -ge $MaxAiFootprintBytes) {
    $Failures.Add("Combined AI compressed footprint ($AiCompressedTotal B) exceeds 35 MB budget ($MaxAiFootprintBytes B).")
}

if ($Failures.Count -gt 0) {
    foreach ($f in $Failures) {
        Write-Error $f
    }
    exit 1
}

Write-Host '[PASS] APK budget check passed.'
exit 0
