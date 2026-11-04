@echo off
REM Stop the relay and its tunnel.
setlocal
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\stop.ps1" %*
set "CODE=%ERRORLEVEL%"
popd
echo.
pause
exit /b %CODE%
