$dumpbin = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\dumpbin.exe'
$libexe  = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\lib.exe'
$outDir  = 'C:\Users\death\repo\theater scene(rust)\Trust\obs_libs'

if (!(Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

function Generate-Lib($dllName) {
    $dllPath = "C:\Program Files\obs-studio\bin\64bit\$dllName.dll"
    $base = [System.IO.Path]::GetFileNameWithoutExtension($dllName)
    $defPath = "$outDir\$base.def"
    $libPath = "$outDir\$base.lib"
    
    $output = & $dumpbin /exports $dllPath
    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add("EXPORTS")
    $started = $false
    foreach ($line in $output) {
        if ($line -match 'ordinal\s+hint\s+RVA\s+name') {
            $started = $true
            continue
        }
        if ($line -match 'Summary') {
            break
        }
        if ($started) {
            $trimmed = $line.Trim()
            if ($trimmed.Length -eq 0) { continue }
            $parts = $trimmed -split '\s+'
            if ($parts.Count -ge 4) {
                # Symbol is in parts[3]
                $sym = $parts[3]
                # If there's an alias like sym = other_sym, take just the sym
                $lines.Add($sym)
            }
        }
    }
    $lines | Set-Content -Path $defPath -Encoding ASCII
    & $libexe /def:$defPath /machine:x64 /out:$libPath | Out-Null
    Write-Host "Generated $libPath (size: $((Get-Item $libPath).Length) bytes, $($lines.Count - 1) symbols)"
}

Generate-Lib "obs-frontend-api"
Generate-Lib "Qt6Core"
Generate-Lib "Qt6Gui"
Generate-Lib "Qt6Widgets"
