@echo off
rem ===================================================================
rem  launch-editor.bat — starts the an1mbyt3 editor in dev mode.
rem
rem  Double-click this file (or run it from a terminal). It:
rem    1. frees port 1420 if a previous dev server was left running
rem       (otherwise Vite can't start and the app never opens),
rem    2. sets this machine's required environment:
rem         WGPU_BACKEND=dx12   - the Vulkan backend crashes wgpu here
rem         CARGO_BUILD_JOBS=2  - keeps Rust builds from running out of memory
rem    3. rebuilds the export player (public/player) against the current engine,
rem    4. runs `npm run tauri dev` in apps\editor (Vite + the native window).
rem
rem  Close this console window (or press Ctrl+C) to stop the editor.
rem ===================================================================

setlocal
cd /d "%~dp0apps\editor"

echo Freeing port 1420 (stale dev server, if any)...
powershell -NoProfile -Command "Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }"

set WGPU_BACKEND=dx12
set CARGO_BUILD_JOBS=2

rem  Rebuild the exported-game player so it always matches the current
rem  engine glue (wasm-pkg). A stale player makes exported games fail with
rem  "function import requires a callable" after any engine change.
echo Rebuilding the export player...
call npm run build:player

echo Starting the an1mbyt3 editor...
call npm run tauri dev

echo.
echo The editor has exited.
pause
