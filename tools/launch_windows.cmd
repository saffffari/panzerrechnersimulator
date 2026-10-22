@echo off
rem Double-click entry: fresh Canyon v2 winter run. Pass -Mode menu / -Windowed / -Build through.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0launch_windows.ps1" %*
