# Builds the release zip, dist\theater-scene-<version>-windows-x64.zip:
#   theater-scene\bin\64bit   the plugin (theater-scene.dll) and its Qt shim (trust_qt.dll)
#   theater-scene\data        assets, filters, scripts (only the files git tracks)
# People extract it into C:\ProgramData\obs-studio\plugins.
# Build first: cd Trust; cargo build --release; .\build_trust_qt.ps1
param([string]$Version = "0.1.0")
$ErrorActionPreference = 'Stop'

$root = $PSScriptRoot
$work = "$root\dist\stage"
$stage = "$work\theater-scene"
foreach ($f in @("$root\Trust\target\release\trust.dll", "$root\Trust\target\release\trust_qt.dll")) {
    if (!(Test-Path -LiteralPath $f)) { throw "$f not found - build it first." }
}
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Force "$stage\bin\64bit", "$stage\data" | Out-Null
Copy-Item -LiteralPath "$root\Trust\target\release\trust.dll" "$stage\bin\64bit\theater-scene.dll"
Copy-Item -LiteralPath "$root\Trust\target\release\trust_qt.dll" "$stage\bin\64bit\trust_qt.dll"

# Only what git tracks: no caches, no files of yours
foreach ($pair in @(@("Trust/assets", "assets"), @("filters", "filters"), @("scripts", "scripts"))) {
    foreach ($file in (git -C $root -c core.quotepath=off ls-files -- $pair[0])) {
        $dest = Join-Path "$stage\data\$($pair[1])" $file.Substring($pair[0].Length + 1)
        New-Item -ItemType Directory -Force -Path (Split-Path -LiteralPath $dest) | Out-Null
        Copy-Item -LiteralPath (Join-Path $root $file) -Destination $dest
    }
}
Copy-Item -LiteralPath "$root\LICENSE" "$stage\LICENSE.txt"
# The version, which the Theater Installer reads to offer updates
Set-Content -LiteralPath "$stage\version.txt" -Value $Version -Encoding ascii

$zip = "$root\dist\theater-scene-$Version-windows-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
# Entries named with / between folders, as zip tools expect (in Windows
# PowerShell, Compress-Archive and ZipFile.CreateFromDirectory put \)
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$archive = [System.IO.Compression.ZipFile]::Open($zip, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    $base = $work.Length + 1
    foreach ($file in Get-ChildItem -LiteralPath $stage -Recurse -File) {
        $name = $file.FullName.Substring($base).Replace('\', '/')
        [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $file.FullName, $name, [System.IO.Compression.CompressionLevel]::Optimal)
    }
} finally { $archive.Dispose() }
Remove-Item $work -Recurse -Force
Write-Host "Made $zip"
