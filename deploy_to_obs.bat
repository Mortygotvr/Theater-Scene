@echo off
setlocal enabledelayedexpansion
title Deploy Theater Scene to OBS Studio

:: Check for administrative rights
net session >nul 2>&1
if not %errorLevel% == 0 (
    echo ========================================================
    echo Requesting Administrator permissions to deploy to OBS...
    echo (Click 'Yes' on the Windows UAC prompt)
    echo ========================================================
    powershell -NoProfile -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)

echo [1/3] Copying theater-scene.dll and trust_qt.dll to OBS Plugins...
copy /Y "%~dp0theater-scene.dll" "C:\Program Files\obs-studio\obs-plugins\64bit\theater-scene.dll"
copy /Y "%~dp0trust.dll" "C:\Program Files\obs-studio\obs-plugins\64bit\trust.dll"
copy /Y "%~dp0trust_qt.dll" "C:\Program Files\obs-studio\obs-plugins\64bit\trust_qt.dll"

echo [2/3] Setting up plugin data...
if not exist "C:\Program Files\obs-studio\data\obs-plugins\theater-scene" mkdir "C:\Program Files\obs-studio\data\obs-plugins\theater-scene"
if exist "%~dp0Trust\assets" xcopy /E /I /Y "%~dp0Trust\assets" "C:\Program Files\obs-studio\data\obs-plugins\theater-scene\assets" >nul
if exist "%~dp0filters" xcopy /E /I /Y "%~dp0filters" "C:\Program Files\obs-studio\data\obs-plugins\theater-scene\filters" >nul

echo [3/3] Granting future write permissions so IDE can auto-update without prompts...
icacls "C:\Program Files\obs-studio\obs-plugins\64bit\theater-scene.dll" /grant "Users:F" >nul
icacls "C:\Program Files\obs-studio\obs-plugins\64bit\trust.dll" /grant "Users:F" >nul
icacls "C:\Program Files\obs-studio\obs-plugins\64bit\trust_qt.dll" /grant "Users:F" >nul

echo.
echo ========================================================
echo SUCCESS: Theater Scene plugin deployed to OBS Studio!
echo You can now open OBS Studio.
echo ========================================================
echo.
pause
