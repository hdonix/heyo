@echo off
rem Inside the test machine: Heyo's msvc build with its settings drawn by
rem Slint. Copies this directory to the machine's own disk, runs test.ps1
rem there, and leaves what was seen in the directory "out" here: pictures of
rem the screen, what Heyo logged, and test.log, which says what was done.
rem
rem Nothing of the machine's own Heyo is touched: this one keeps its settings
rem and its data in a directory of its own, and is ended by its own number.
set S=%~dp0
set L=%USERPROFILE%\heyo-slint-settings
set O=%S%out
if not exist "%O%" mkdir "%O%"
del /q "%O%\done.txt" 2>nul
robocopy "%S%." "%L%" /e /xd out /njh /njs /ndl /np /r:5 /w:2 >nul
cd /d "%L%"
powershell -NoProfile -ExecutionPolicy Bypass -File "%L%\test.ps1" -Out "%O%"
echo done > "%O%\done.txt"
