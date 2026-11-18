@echo off
REM ==========================================================================
REM  SourceGit-rs - build a Windows portable package
REM
REM  Usage:   build-windows-portable.bat [x64^|arm64] [--test] [--clean]
REM  Output:  dist\SourceGit-rs-<version>-win-<arch>-portable\
REM           dist\SourceGit-rs-<version>-win-<arch>-portable.zip
REM
REM  Requirements:
REM    - Rust (https://rustup.rs)
REM    - Visual Studio 2019/2022 Build Tools, workload "Desktop development with C++"
REM      (MSVC linker). The script always builds with the MSVC toolchain, even when
REM      the default rustup toolchain is windows-gnu.
REM  Runtime: Git for Windows >= 2.25.1 on PATH.
REM ==========================================================================
setlocal EnableExtensions
cd /d "%~dp0"

set "ARCH=x64"
set "RUN_TEST=0"
set "CLEAN=0"
for %%A in (%*) do (
    if /I "%%~A"=="x64"     set "ARCH=x64"
    if /I "%%~A"=="arm64"   set "ARCH=arm64"
    if /I "%%~A"=="--test"  set "RUN_TEST=1"
    if /I "%%~A"=="--clean" set "CLEAN=1"
)
set "TARGET=x86_64-pc-windows-msvc"
set "VSCOMP=Microsoft.VisualStudio.Component.VC.Tools.x86.x64"
if /I "%ARCH%"=="arm64" set "TARGET=aarch64-pc-windows-msvc"
if /I "%ARCH%"=="arm64" set "VSCOMP=Microsoft.VisualStudio.Component.VC.Tools.ARM64"

REM ---- 1. Rust ---------------------------------------------------------------
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] cargo not found. Install Rust from https://rustup.rs and reopen this window.
    goto :fail
)
where rustup >nul 2>nul
if errorlevel 1 (
    echo [ERROR] rustup not found. This script needs rustup to pick the MSVC toolchain.
    goto :fail
)

REM ---- 2. Use the MSVC toolchain (the GNU host toolchain breaks build scripts) --
set "HOST="
for /f "tokens=2" %%H in ('rustc -vV ^| findstr /B /C:"host:"') do set "HOST=%%H"
echo [INFO ] default rustc host: %HOST%

set "TOOLCHAIN=stable-x86_64-pc-windows-msvc"
if /I "%PROCESSOR_ARCHITECTURE%"=="ARM64" set "TOOLCHAIN=stable-aarch64-pc-windows-msvc"

rustup toolchain list | findstr /I /C:"%TOOLCHAIN:stable-=%" >nul
if errorlevel 1 (
    echo [INFO ] installing Rust toolchain %TOOLCHAIN% ...
    rustup toolchain install %TOOLCHAIN% --profile minimal
    if errorlevel 1 goto :fail
)
rustup target add --toolchain %TOOLCHAIN% %TARGET% >nul 2>nul
set "CARGO=cargo +%TOOLCHAIN%"

REM Do not let a mingw/llvm-mingw linker from PATH or env leak into the MSVC build.
set "RUSTFLAGS="
set "CARGO_BUILD_TARGET="
set "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER="
set "CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER="

REM ---- 3. MSVC linker (Visual Studio Build Tools) ---------------------------
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" goto :no_msvc
set "VSPATH="
for /f "usebackq delims=" %%P in (`"%VSWHERE%" -latest -products * -requires %VSCOMP% -property installationPath`) do set "VSPATH=%%P"
if not defined VSPATH goto :no_msvc
echo [INFO ] MSVC build tools: %VSPATH%

where git >nul 2>nul
if errorlevel 1 echo [WARN ] git not found on PATH. The build works, but the app needs Git for Windows to run.

REM ---- 4. Version from Cargo.toml -------------------------------------------
set "VERSION="
for /f "tokens=2 delims==" %%V in ('findstr /R /B /C:"version *=" Cargo.toml') do if not defined VERSION set "VERSION=%%~V"
set "VERSION=%VERSION: =%"
set VERSION=%VERSION:"=%
if not defined VERSION set "VERSION=0.0.0"

set "NAME=SourceGit-rs-%VERSION%-win-%ARCH%-portable"
set "DIST=%~dp0dist"
set "OUT=%DIST%\%NAME%"

echo.
echo === SourceGit-rs %VERSION% ^| %TOOLCHAIN% ^| target %TARGET% ===
echo.

REM ---- 5. Build -------------------------------------------------------------
if "%CLEAN%"=="1" %CARGO% clean
%CARGO% build --release --locked --target %TARGET%
if errorlevel 1 goto :fail

set "EXE=%~dp0target\%TARGET%\release\sourcegit-rs.exe"
if not exist "%EXE%" (
    echo [ERROR] build output not found: %EXE%
    goto :fail
)

REM ---- 6. Optional self-test (only possible when the exe matches this CPU) --
if not "%RUN_TEST%"=="1" goto :package
if not exist "%DIST%" mkdir "%DIST%"
echo.
echo === Running self-test ===
start "" /wait "%EXE%" --self-test "%DIST%\self-test.json"
if errorlevel 1 (
    echo [ERROR] self-test failed, see %DIST%\self-test.json
    goto :fail
)
echo [OK   ] self-test passed: %DIST%\self-test.json

:package
REM ---- 7. Portable folder ---------------------------------------------------
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%\data" || goto :fail
copy /y "%EXE%" "%OUT%\SourceGit-rs.exe" >nul || goto :fail
copy /y "README.md" "%OUT%\README.md" >nul
if exist "..\..\LICENSE" copy /y "..\..\LICENSE" "%OUT%\LICENSE.txt" >nul
> "%OUT%\data\README.txt" echo This folder makes SourceGit-rs portable: settings are stored here instead of %%APPDATA%%\SourceGit-rs. Delete it to use %%APPDATA%% again.

REM ---- 8. Zip -----------------------------------------------------------------
if exist "%DIST%\%NAME%.zip" del /q "%DIST%\%NAME%.zip"
powershell -NoProfile -ExecutionPolicy Bypass -Command "Compress-Archive -Path '%OUT%' -DestinationPath '%DIST%\%NAME%.zip' -CompressionLevel Optimal"
if errorlevel 1 echo [WARN ] zip step failed; the portable folder is still available.

echo.
echo === Done ===
echo Folder : %OUT%
if exist "%DIST%\%NAME%.zip" echo Zip    : %DIST%\%NAME%.zip
echo.
endlocal
exit /b 0

:no_msvc
echo.
echo [ERROR] MSVC build tools not found (needed component: %VSCOMP%).
echo         Install Visual Studio Build Tools with the C++ workload, then rerun this script:
echo.
echo   winget install Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
echo.
echo         or download: https://visualstudio.microsoft.com/visual-cpp-build-tools/
echo         (choose "Desktop development with C++"; for arm64 also tick "MSVC ARM64 build tools").
goto :fail

:fail
echo.
echo === Build FAILED ===
endlocal
exit /b 1
