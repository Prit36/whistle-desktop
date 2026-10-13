@echo off
if not exist "%~dp0dist\Whistle\Whistle.exe" (
  echo Build the app first: python scripts\package.py
  pause
  exit /b 1
)
start "" "%~dp0dist\Whistle\Whistle.exe" %*
