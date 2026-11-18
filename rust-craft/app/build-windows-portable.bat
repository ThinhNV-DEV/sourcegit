@echo off
REM ==========================================================================
REM  SourceGit-rs - build a Windows portable package
REM
REM  Usage:   build-windows-portable.bat [x64|arm64] [--test]
REM  Output:  dist\SourceGit-rs-<version>-win-<arch>-portable\
REM           dist\SourceGit-rs-<version>-win-<arch>-portable.zip
REM
REM  Requirements: Rust (https://rustup.rs) with the MSVC toolchain
REM                (Visual Studio Build Tools, "Desktop development with C++").
REM  Runtime:      Git for Windows >= 2.25.1 on PATH.
REM ==========================================================================
setlocal EnableExtensions EnableDelayedExpansion
cd /d "%~dp0"

set "ARCH=x64"
set "RUN_TEST=0"
for %%A in (%*) do (
    if /I "%%~A"=="x64"    set "ARCH=x64"
    if /I "%%~A"=="arm64"  set "ARCH=arm64"
    if /I "%%~A"=="--test" set "RUN_TEST=1"
)
if /I "%ARCH%"=="arm64" (
    set "TARGET=aarch64-pc-windows-msvc"
) else (
    set "TARGET=x86_64-pc-windows-msvc"
)

REM ---- check tools ---------------------------------------------------------
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found. Install Rust from https://rustup.rs and reopen this window.
    goto :fail
)
where git >nul 2>nul
if errorlevel 1 (
    echo [WARN ] git not found on PATH. The build works, but the app needs Git for Windows to run.
)

REM ---- version from Cargo.toml ---------------------------------------------
set "VERSION="
for /f "tokens=2 delims==" %%V in ('findstr /R /B /C:"version *=" Cargo.toml') do (
    if not defined VERSION set "VERSION=%%~V"
)
set "VERSION=%VERSION: =%"
set "VERSION=%VERSION:"=%"
if not defined VERSION set "VERSION=0.0.0"

set "NAME=SourceGit-rs-%VERSION%-win-%ARCH%-portable"
set "DIST=%~dp0dist"
set "OUT=%DIST%\%NAME%"

echo.
echo === SourceGit-rs %VERSION% ^| target %TARGET% ===
echo.

REM ---- build ---------------------------------------------------------------
rustup target add %TARGET% >nul 2>nul
cargo build --release --locked --target %TARGET%
if errorlevel 1 (
    echo [INFO ] --locked failed, retrying without it...
    cargo build --release --target %TARGET%
    if errorlevel 1 goto :fail
)

set "EXE=%~dp0target\%TARGET%\release\sourcegit-rs.exe"
if not exist "%EXE%" (
    echo [ERROR] build output not found: %EXE%
    goto :fail
)

REM ---- optional self-test --------------------------------------------------
if "%RUN_TEST%"=="1" (
    echo.
    echo === Running self-test ===
    "%EXE%" --self-test "%DIST%\self-test.json"
    if errorlevel 1 (
        echo [ERROR] self-test failed, see %DIST%\self-test.json
        goto :fail
    )
    echo [OK   ] self-test passed
)

REM ---- assemble portable folder -------------------------------------------
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%\data" || goto :fail

copy /y "%EXE%" "%OUT%\SourceGit-rs.exe" >nul || goto :fail
copy /y "README.md" "%OUT%\README.md" >nul
if exist "..\..\LICENSE" copy /y "..\..\LICENSE" "%OUT%\LICENSE.txt" >nul

> "%OUT%\data\README.txt" (
    echo This folder makes SourceGit-rs portable.
    echo Settings ^(preference.json, repository list, open tabs^) are stored here
    echo instead of %%APPDATA%%\SourceGit-rs. Delete this folder to use %%APPDATA%% again.
)

REM ---- zip -----------------------------------------------------------------
if exist "%DIST%\%NAME%.zip" del /q "%DIST%\%NAME%.zip"
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "Compress-Archive -Path '%OUT%' -DestinationPath '%DIST%\%NAME%.zip' -CompressionLevel Optimal"
if errorlevel 1 (
    echo [WARN ] zip step failed; the portable folder is still available.
)

echo.
echo === Done ===
echo Folder : %OUT%
if exist "%DIST%\%NAME%.zip" echo Zip    : %DIST%\%NAME%.zip
echo.
endlocal
exit /b 0

:fail
echo.
echo === Build FAILED ===
endlocal
exit /b 1
