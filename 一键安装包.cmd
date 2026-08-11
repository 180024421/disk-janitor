@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0pack-setup.ps1"
if errorlevel 1 exit /b 1
