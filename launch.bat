@echo off
rem ============================================================================
rem  PROJECT MILKY WAY (galaxia) — Windows launcher (target: TITAN RTX/Vulkan)
rem
rem  Usage:
rem    launch.bat                 auto scene: t0_orion_m13 if the baked T0
rem                               octree exists, else orion_wide (bright subset)
rem    launch.bat SCENE           orion_wide | orion_belt | betelgeuse |
rem                               pleiades | t0_orion_m13
rem    launch.bat SCENE debug     use the debug build (default: release)
rem
rem  The windowed app opens a winit window with the egui debug overlay (F1
rem  toggles it per overlay help). For headless shots/benches/fly-tests run
rem  the exe directly, e.g.:
rem    target\release\galaxia.exe --scene t0_orion_m13 --screenshot shot.png
rem    target\release\galaxia.exe --scene orion_wide --bench
rem    target\release\galaxia.exe --scene t0_orion_m13 --fly-test 60,50
rem
rem  t0_* scenes need the baked octree (gitignored by design). One-time prep
rem  (~2 h fetch + ~1 min bake, resumable):
rem    python etl\fetch_cone_exact.py        (repeat in chunks if interrupted)
rem    python etl\bake_literature_bin.py
rem    python etl\octree_build.py
rem  Needs python with numpy + healpy on PATH.
rem ============================================================================
setlocal enabledelayedexpansion
cd /d "%~dp0"

set "SCENE=%~1"
set "PROFILE=%~2"
if "%PROFILE%"=="" set "PROFILE=release"
if /i "%PROFILE%"=="debug" (set "BUILDARGS=-p galaxia-app") else (set "BUILDARGS=--release -p galaxia-app")

where cargo >nul 2>&1
if errorlevel 1 (
    echo [launch] ERROR: cargo not found. Install Rust from https://rustup.rs
    echo          and reopen this terminal.
    pause
    exit /b 1
)

rem --- scene auto-pick -------------------------------------------------------
if "%SCENE%"=="" (
    if exist "etl\build\t0\tree_meta.json" (set "SCENE=t0_orion_m13") else (set "SCENE=orion_wide")
)

rem --- t0 data check ----------------------------------------------------------
set "T0OK=1"
if not exist "etl\build\t0\tree_meta.json" set "T0OK=0"
if "%T0OK%"=="0" (
    if /i "%SCENE:~0,2%"=="t0" (
        echo [launch] NOTE: baked T0 octree not found at etl\build\t0\ —
        echo          t0_* scenes need it. Falling back to orion_wide.
        echo          One-time prep:  python etl\fetch_cone_exact.py
        echo                        python etl\bake_literature_bin.py
        echo                        python etl\octree_build.py
        set "SCENE=orion_wide"
    )
)

rem --- build if needed --------------------------------------------------------
set "BIN=target\%PROFILE%\galaxia.exe"
if not exist "%BIN%" (
    echo [launch] First run: building %PROFILE% profile with default features
    echo          (Vulkan on Windows, per ADR-0010). This takes several minutes.
    cargo build %BUILDARGS%
    if errorlevel 1 (
        echo [launch] ERROR: build failed.
        pause
        exit /b 1
    )
)

rem --- launch -----------------------------------------------------------------
echo [launch] scene=%SCENE% profile=%PROFILE% bin=%BIN%
"%BIN%" --scene "%SCENE%"
if errorlevel 1 (
    echo.
    echo [launch] The app exited with an error. If this is the first windowed
    echo          run on this machine, treat it as INTEGRATION (ADR-0010):
    echo          check the Vulkan driver is current and re-run.
    pause
)
endlocal
