@echo off
setlocal
cd /d "%~dp0"
start "" "%~dp0dist\PinShot\PinShot.exe" %*
