@echo off
rem A small front for sonic-tui on Windows: put this folder on the path and
rem type "sonic-tui" (or "SONIC-TUI") in a command prompt.
rem
rem It runs the copy in this folder when it has been built, and the installed
rem one otherwise.

setlocal
set "HERE=%~dp0"

if exist "%HERE%target\release\sonic-tui.exe" goto :local
if exist "%HERE%target\debug\sonic-tui.exe" goto :local

where sonic-tui >nul 2>&1
if %errorlevel%==0 goto :installed

echo sonic-tui is not built yet.  Build it with:  cargo install --path "%HERE%" 1>&2
exit /b 1

:local
"%HERE%target\release\sonic-tui.exe" %*
exit /b %errorlevel%

:installed
sonic-tui %*
exit /b %errorlevel%
