@echo off
rem Inside the test machine: stops Heyo, takes the latest build from the share
rem (running it from the share itself gets a stale copy) and starts it hidden.
rem Alt+Space then shows it; its log is %LOCALAPPDATA%\heyo\heyo.log.
taskkill /f /im heyo.exe >nul 2>&1
robocopy \\host.lan\Data\src-tauri\target\heyo-windows "%USERPROFILE%\heyo" /mir /njh /njs /ndl /np /r:5 /w:2
start "" "%USERPROFILE%\heyo\heyo.exe" --hidden
echo Heyo started.
