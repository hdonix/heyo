@echo off
rem Inside the test machine: shows the end of Heyo's log.
powershell -NoProfile -Command "Get-Content $env:LOCALAPPDATA\heyo\heyo.log -Tail 25"
