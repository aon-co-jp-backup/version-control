@echo off
rem version-control --auto for Task Scheduler (Windows). ASCII only on purpose.
rem Usage: run-auto.cmd [extra args]   e.g. run-auto.cmd --only open-fudousan   (default: --auto --dry-run --no-api)
rem Needs on PATH: cargo, git, gh (logged in), npm, bash (Git for Windows). Output: run-auto.log next to config.toml.
set "PATH=%USERPROFILE%\.cargo\bin;C:\Program Files\Git\bin;C:\Program Files\Git\usr\bin;C:\Program Files\GitHub CLI;C:\Program Files\nodejs;%PATH%"
cd /d "%~dp0.."
set "ARGS=%*"
if "%ARGS%"=="" set "ARGS=--auto --dry-run --no-api"
echo ===== %DATE% %TIME% version-control %ARGS% >> run-auto.log
target\release\version-control.exe %ARGS% >> run-auto.log 2>&1
echo exit=%ERRORLEVEL% >> run-auto.log
