@echo off
REM Double-click to start the relay and get a public install URL.
REM   start.cmd            foreground, Ctrl+C to stop
REM   start.cmd -Background detached, logs in logs\relay.log
REM   start.cmd -Open      also open the install page
setlocal
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\start.ps1" %*
set "CODE=%ERRORLEVEL%"
popd
echo.
if not "%CODE%"=="0" echo Start failed with exit code %CODE%.
pause
exit /b %CODE%
