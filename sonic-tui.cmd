@echo off
rem A small front for sonic-tui on Windows.
rem
rem It builds the player first, so the program is never out of date, and then
rem runs it. ".\sonic-tui" in this folder, and "sonic" from anywhere, both come
rem through here.

setlocal
set "HERE=%~dp0"
set "EXE=%HERE%target\release\sonic-tui.exe"

pushd "%HERE%"
cargo build --release --quiet
if errorlevel 1 (
  popd
  echo sonic-tui: the build failed 1>&2
  exit /b 1
)
popd

if not exist "%EXE%" (
  echo sonic-tui: there is no build 1>&2
  exit /b 1
)

"%EXE%" %*
exit /b %errorlevel%
