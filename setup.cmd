@echo off
REM Double-click to install dependencies, cloudflared and the .env file.
setlocal
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\setup.ps1" %*
set "CODE=%ERRORLEVEL%"
popd
echo.
if not "%CODE%"=="0" echo Setup failed with exit code %CODE%.
pause
exit /b %CODE%
