$progDataDest = "C:\ProgramData\obs-studio\plugins\theater-scene\bin\64bit"
$appDataDest = "$env:APPDATA\obs-studio\plugins\theater-scene\bin\64bit"

foreach ($dir in @($progDataDest, $appDataDest)) {
    if (!(Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
}

$releaseDll = "$PSScriptRoot\Trust\target\release\trust.dll"
$releaseQtDll = "$PSScriptRoot\Trust\target\release\trust_qt.dll"

# 1. Root repo (clean build output)
Copy-Item $releaseDll "$PSScriptRoot\theater-scene.dll" -Force
Copy-Item $releaseQtDll "$PSScriptRoot\trust_qt.dll" -Force

# 2. C:\ProgramData plugins (primary OBS plugin directory on Windows)
Copy-Item $releaseDll "$progDataDest\theater-scene.dll" -Force
Copy-Item $releaseQtDll "$progDataDest\trust_qt.dll" -Force

# 3. %APPDATA% plugins (user plugin directory)
Copy-Item $releaseDll "$appDataDest\theater-scene.dll" -Force
Copy-Item $releaseQtDll "$appDataDest\trust_qt.dll" -Force

# Remove any old redundant trust.dll from plugin folders to avoid confusion
Remove-Item "$progDataDest\trust.dll" -Force -ErrorAction SilentlyContinue
Remove-Item "$appDataDest\trust.dll" -Force -ErrorAction SilentlyContinue

# Sync assets and filters
foreach ($baseData in @("C:\ProgramData\obs-studio\plugins\theater-scene\data", "$env:APPDATA\obs-studio\plugins\theater-scene\data")) {
    if (!(Test-Path $baseData)) {
        New-Item -ItemType Directory -Path $baseData -Force | Out-Null
    }
    # The folders' contents: copying a folder onto one that already exists
    # puts it inside (data\assets\assets)
    foreach ($pair in @(@("$PSScriptRoot\Trust\assets", "$baseData\assets"), @("$PSScriptRoot\filters", "$baseData\filters"))) {
        if (Test-Path $pair[0]) {
            New-Item -ItemType Directory -Path $pair[1] -Force | Out-Null
            Copy-Item "$($pair[0])\*" $pair[1] -Recurse -Force
        }
    }
}

Write-Host "=== C:\ProgramData\obs-studio\plugins\theater-scene\bin\64bit ==="
Get-ChildItem -Path $progDataDest | Format-Table Name, LastWriteTime, Length
# (Theater Reader deploys itself: deploy.ps1 in its own repo)
