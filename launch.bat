@echo off
rem ============================================================================
rem  PROJECT MILKY WAY - galaxia - Windows launcher. Target: TITAN RTX/Vulkan.
rem
rem  Usage:
rem    launch.bat                auto scene: t0_orion_m13 if the baked T0
rem                              octree exists, else orion_wide bright subset
rem    launch.bat SCENE          orion_wide / orion_belt / betelgeuse /
rem                              pleiades / t0_orion_m13
rem    launch.bat SCENE debug    use the debug build. Default profile: release
rem
rem  Windowed app = winit window + egui debug overlay. Headless usage, run the
rem  exe directly from this folder:
rem    target\release\galaxia.exe --scene t0_orion_m13 --screenshot shot.png
rem    target\release\galaxia.exe --scene orion_wide --bench
rem    target\release\galaxia.exe --scene t0_orion_m13 --fly-test 60,50
rem
rem  t0 scenes need the baked octree, gitignored by design. One-time prep,
rem  about 2 h fetch plus about 1 min bake, resumable, needs python with
rem  numpy and healpy:
rem    python etl\fetch_cone_exact.py
rem    python etl\bake_literature_bin.py
rem    python etl\octree_build.py
rem ============================================================================
setlocal enableextensions
cd /d "%~dp0"
title PROJECT MILKY WAY - galaxia launcher

set "SCENE=%~1"
set "PROFILE=%~2"
if not defined PROFILE set "PROFILE=release"
set "BUILDARGS=--release -p galaxia-app"
if /i "%PROFILE%"=="debug" set "BUILDARGS=-p galaxia-app"
echo [launch] args: %*

where cargo >nul 2>&1 || goto :nocargo

rem --- scene auto-pick when no argument. Plain lines only, no blocks. -------
if not "%SCENE%"=="" goto :scene_set
if exist "etl\build\t0\tree_meta.json" set "SCENE=t0_orion_m13"
if not "%SCENE%"=="" goto :scene_set
set "SCENE=orion_wide"
:scene_set
echo [launch] scene resolved to: %SCENE%

rem --- scene whitelist -------------------------------------------------------
for %%s in (orion_wide orion_belt betelgeuse pleiades t0_orion_m13) do if /i "%SCENE%"=="%%s" goto :scene_ok
goto :badscene
:scene_ok

rem --- t0 data guard ----------------------------------------------------------
if exist "etl\build\t0\tree_meta.json" goto :have_t0
if /i "%SCENE:~0,2%"=="t0" goto :not0
:have_t0

rem --- build on first run ------------------------------------------------------
set "BIN=target\%PROFILE%\galaxia.exe"
if exist "%BIN%" goto :run
echo [launch] First run: building the %PROFILE% profile with default features,
echo          i.e. Vulkan on Windows per ADR-0010. This takes several minutes.
cargo build %BUILDARGS% || goto :buildfail

:run
echo [launch] starting scene=%SCENE% profile=%PROFILE%
"%BIN%" --scene "%SCENE%"
if errorlevel 1 goto :appfail
goto :eof

:nocargo
echo [launch] ERROR: cargo not found on PATH.
echo          Install Rust from https://rustup.rs then reopen this terminal.
pause
exit /b 1

:not0
echo [launch] NOTE: baked T0 octree not found under etl\build\t0 -
echo          t0 scenes need it. Falling back to orion_wide for now.
echo          One-time prep:  python etl\fetch_cone_exact.py
echo                        python etl\bake_literature_bin.py
echo                        python etl\octree_build.py
set "SCENE=orion_wide"
goto :have_t0

:badscene
echo [launch] ERROR: unknown scene "%SCENE%".
echo          Valid: orion_wide, orion_belt, betelgeuse, pleiades, t0_orion_m13
pause
exit /b 1

:buildfail
echo [launch] ERROR: cargo build failed. See the compiler output above.
pause
exit /b 1

:appfail
echo.
echo [launch] The app exited with an error code.
echo          First windowed run on a machine is INTEGRATION per ADR-0010:
echo          update the NVIDIA driver and re-run before suspecting the app.
pause
exit /b 1
