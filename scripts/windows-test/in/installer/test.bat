@echo off
rem Inside the test machine: tries Heyo's installer. Copies this directory
rem to the machine's own disk (what is run from the share may be a stale
rem copy), runs test.ps1 there, and leaves what was seen in the directory
rem "out" here; done.txt is written last. scripts/windows-test/installer.sh,
rem on the Linux side, puts this beside an installer.
set S=%~dp0
set L=%USERPROFILE%\heyo-installer-test
set O=%S%out
if not exist "%O%" mkdir "%O%"
del /q "%O%\done.txt" 2>nul
robocopy "%S%." "%L%" /mir /xd out /njh /njs /ndl /np /r:5 /w:2 >nul
cd /d "%L%"
powershell -NoProfile -ExecutionPolicy Bypass -File "%L%\test.ps1" -Out "%O%" > "%O%\powershell.txt" 2>&1
echo powershell ended with %errorlevel% >> "%O%\powershell.txt"
echo done > "%O%\done.txt"
