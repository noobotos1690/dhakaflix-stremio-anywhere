@echo off
REM Commit and push this project to your own GitHub fork.
REM The first run opens a browser window to sign in to GitHub (one time).
setlocal
pushd "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\push-to-github.ps1" %*
set "CODE=%ERRORLEVEL%"
popd
echo.
pause
exit /b %CODE%
