$cl = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\cl.exe'
$link = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe'
$msvcInclude = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\include'
$msvcLib = 'C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\lib\x64'
$sdkInclude = (Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\Include' | Sort-Object Name -Descending | Select-Object -First 1).FullName
$sdkLib = (Get-ChildItem -Path 'C:\Program Files (x86)\Windows Kits\10\Lib' | Sort-Object Name -Descending | Select-Object -First 1).FullName

$srcDir = "$PSScriptRoot\src"
$outDir = "$PSScriptRoot\target\release"
if (!(Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir -Force | Out-Null }

Write-Host "Compiling qt_dialog_win.cpp..."
& $cl /c /O2 /MD /EHsc /I "$msvcInclude" /I "$sdkInclude\ucrt" /I "$sdkInclude\um" /I "$sdkInclude\shared" /Fo:"$outDir\qt_dialog_win.obj" "$srcDir\qt_dialog_win.cpp"
if ($LASTEXITCODE -ne 0) {
    Write-Error "Compilation failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

Write-Host "Linking trust_qt.dll with delayload..."
& $link /DLL /OUT:"$outDir\trust_qt.dll" "$outDir\qt_dialog_win.obj" /LIBPATH:"$msvcLib" /LIBPATH:"$sdkLib\ucrt\x64" /LIBPATH:"$sdkLib\um\x64" /LIBPATH:"$PSScriptRoot\obs_libs" Qt6Core.lib Qt6Gui.lib Qt6Widgets.lib obs-frontend-api.lib /DELAYLOAD:Qt6Core.dll /DELAYLOAD:Qt6Widgets.dll /DELAYLOAD:Qt6Gui.dll /DELAYLOAD:obs-frontend-api.dll delayimp.lib
if ($LASTEXITCODE -ne 0) {
    Write-Error "Linking failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

# Copy to root repo and Trust root for runtime lookup
Copy-Item "$outDir\trust_qt.dll" "$PSScriptRoot\..\trust_qt.dll" -Force
Copy-Item "$outDir\trust_qt.dll" "$PSScriptRoot\trust_qt.dll" -Force

Write-Host "SUCCESS: Created $outDir\trust_qt.dll (size: $((Get-Item "$outDir\trust_qt.dll").Length) bytes)"
