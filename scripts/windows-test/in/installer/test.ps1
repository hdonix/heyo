# Inside the test machine: tries Heyo's installer (test.bat runs this).
# Installs it without a window, starts what it installed, asks it for its
# settings, and uninstalls it, writing down what was seen in the directory
# given: test.log says what was done and what went wrong, and names the
# other files.
#
# The installed Heyo is started by this script and ended by the number it
# was given when started. No process is ended by its name here. (The
# installer itself ends a Heyo it finds running from the directory it
# installs into.)
#
# The functions here have names PowerShell has no command or alias of: an
# alias wins over a function of the same name.
param([string]$Out)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot

function Write-TestLog($text) { Add-Content -Path "$Out\test.log" -Value ("{0:HH:mm:ss} {1}" -f (Get-Date), $text) }
# One step of the test: an error in it is written down, and the next is tried.
function Invoke-TestStep($name, $work) {
  Write-TestLog "--- $name"
  try { & $work } catch {
    Write-TestLog "${name}: STOPPED by an error: $($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber): $($_.InvocationInfo.Line.Trim())"
  }
}
# The screen as Windows composed it.
function Save-TestPicture($name) {
  Add-Type -AssemblyName System.Windows.Forms, System.Drawing
  $screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $picture = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
  $graphics = [System.Drawing.Graphics]::FromImage($picture)
  $graphics.CopyFromScreen($screen.Left, $screen.Top, 0, 0, $picture.Size)
  $picture.Save("$Out\$name.png")
  $graphics.Dispose(); $picture.Dispose()
  Write-TestLog "picture $name.png ($($screen.Width)x$($screen.Height))"
}
# Every file under a directory with its size, or that there is none.
function Save-TestListing($directory, $file) {
  if (Test-Path -LiteralPath $directory) {
    $lines = @(Get-ChildItem -LiteralPath $directory -Recurse -Force | ForEach-Object {
      $size = '<dir>'
      if (-not $_.PSIsContainer) { $size = $_.Length }
      "{0,12} {1}" -f $size, $_.FullName
    })
    Set-Content -Path "$Out\$file" -Value (@("$directory ($($lines.Count) entries)") + $lines)
    Write-TestLog "$directory has $($lines.Count) entries: $file"
  } else {
    Set-Content -Path "$Out\$file" -Value "$directory does not exist"
    Write-TestLog "$directory does not exist"
  }
}
# The processes named heyo.exe, as tasklist says them, and which file each is.
function Save-TestTasks($file) {
  $lines = @(& tasklist.exe /fi 'imagename eq heyo.exe')
  $lines += @(Get-Process -Name heyo -ErrorAction SilentlyContinue | ForEach-Object { "process $($_.Id) is $($_.Path)" })
  Set-Content -Path "$Out\$file" -Value $lines
  Write-TestLog "processes named heyo.exe: $file"
}
# Starts a program and answers its process, with its exit code kept for later.
function Start-TestProgram($program, $arguments) {
  $process = Start-Process -FilePath $program -ArgumentList $arguments -PassThru
  $null = $process.Handle
  return $process
}
function Test-TestAlive($process) { return ($null -ne $process) -and ($null -ne (Get-Process -Id $process.Id -ErrorAction SilentlyContinue)) }
function Stop-TestProcess($process, $what) {
  if (Test-TestAlive $process) {
    cmd /c "taskkill /PID $($process.Id) /T /F >nul 2>&1"
    Write-TestLog "ended $what, process $($process.Id)"
  }
}
function Get-TestPresence($path) { if (Test-Path -LiteralPath $path) { return 'there' } else { return 'NOT there' } }
# What the installer wrote to the registry about itself.
function Get-TestRegistered() { return Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Heyo' -ErrorAction SilentlyContinue }
function Get-TestRegisteredText() {
  $registered = Get-TestRegistered
  if (-not $registered) { return 'nothing' }
  return ($registered.DisplayName + ' ' + $registered.DisplayVersion + ' in ' + $registered.InstallLocation + '; to uninstall: ' + $registered.UninstallString)
}

$script:installed = Join-Path $env:LOCALAPPDATA 'Heyo'
$script:heyo = $null
$script:asker = $null
$shortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Heyo.lnk'
# An installer run without a window makes this one too.
$onDesktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Heyo.lnk'

Invoke-TestStep 'the machine' {
  $system = Get-CimInstance Win32_OperatingSystem
  Write-TestLog ($system.Caption + ' ' + $system.Version + ' ' + $env:PROCESSOR_ARCHITECTURE)
  # If these are there, the Visual C++ runtime is installed on this machine,
  # and a Heyo that starts shows nothing about the DLLs beside it.
  foreach ($dll in 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll') {
    Write-TestLog ("Windows' own " + $dll + ' in System32: ' + (Get-TestPresence (Join-Path $env:SystemRoot ('System32\' + $dll))))
  }
  Save-TestTasks 'tasks-before.txt'
  Save-TestListing $script:installed 'directory-before.txt'
  Write-TestLog ('registered before: ' + (Get-TestRegisteredText))
}

Invoke-TestStep 'install' {
  $setup = @(Get-ChildItem -LiteralPath $here -Filter '*-setup.exe')
  if ($setup.Count -ne 1) { throw "expected one installer beside this script, found $($setup.Count)" }
  Write-TestLog "installer $($setup[0].Name), $($setup[0].Length) bytes"
  $began = Get-Date
  $run = Start-TestProgram $setup[0].FullName @('/S')
  if ($run.WaitForExit(300000)) {
    Write-TestLog "the installer ended with $($run.ExitCode) after $([int]((Get-Date) - $began).TotalSeconds) s"
  } else {
    Write-TestLog "the installer, process $($run.Id), has NOT ended after five minutes; going on"
  }
  Write-TestLog ('registered: ' + (Get-TestRegisteredText))
  $registered = Get-TestRegistered
  if ($registered -and $registered.InstallLocation) { $script:installed = $registered.InstallLocation.Trim('"') }
  Save-TestListing $script:installed 'directory-installed.txt'
  Write-TestLog ('Start menu shortcut ' + $shortcut + ': ' + (Get-TestPresence $shortcut))
  Write-TestLog ('desktop shortcut ' + $onDesktop + ': ' + (Get-TestPresence $onDesktop))
}

Invoke-TestStep 'start' {
  $program = Join-Path $script:installed 'heyo.exe'
  if (-not (Test-Path -LiteralPath $program)) { throw "no $program" }
  # The first Heyo is the one that draws the settings: it has to have this.
  $env:HEYO_SETTINGS = 'slint'
  $script:heyo = Start-TestProgram $program @('--hidden')
  Write-TestLog "started $program --hidden, process $($script:heyo.Id)"
  Start-Sleep 12
  Save-TestTasks 'tasks-started.txt'
  if (Test-TestAlive $script:heyo) {
    Write-TestLog "Heyo is RUNNING after 12 s, using $([int]((Get-Process -Id $script:heyo.Id).WorkingSet64 / 1MB)) MB"
    # Where the runtime's DLLs were loaded from: beside heyo.exe, or Windows' own.
    try {
      $modules = @((Get-Process -Id $script:heyo.Id).Modules | Where-Object { $_.ModuleName -match '^(msvcp140|vcruntime140)' })
      if ($modules.Count -eq 0) { Write-TestLog 'it has loaded no DLL of the Visual C++ runtime' }
      foreach ($module in $modules) { Write-TestLog "it has loaded $($module.FileName)" }
    } catch { Write-TestLog "its loaded DLLs could not be listed: $($_.Exception.Message)" }
  } else {
    $script:heyo.WaitForExit(1000) | Out-Null
    Write-TestLog "Heyo is NOT RUNNING after 12 s; it ended with $($script:heyo.ExitCode) (a Heyo that finds another one running hands over to it and ends with 0: see tasks-before.txt)"
  }
  Save-TestPicture 'started-hidden'
}

Invoke-TestStep 'settings' {
  $program = Join-Path $script:installed 'heyo.exe'
  $began = Get-Date
  $script:asker = Start-TestProgram $program @('--open', 'heyo:settings')
  if ($script:asker.WaitForExit(20000)) {
    Write-TestLog "heyo.exe --open heyo:settings ended with $($script:asker.ExitCode) after $([int]((Get-Date) - $began).TotalMilliseconds) ms"
    $script:asker = $null
  } else {
    Write-TestLog "heyo.exe --open heyo:settings, process $($script:asker.Id), has NOT ended after 20 s (it does when a Heyo is running to hand over to)"
  }
  Start-Sleep 6
  Save-TestPicture 'settings'
  if (Test-TestAlive $script:heyo) { Write-TestLog 'Heyo is still running with its settings asked for' } else { Write-TestLog 'Heyo is NOT running with its settings asked for' }
}

Invoke-TestStep 'the log' {
  $log = Join-Path $env:LOCALAPPDATA 'heyo\heyo.log'
  if (Test-Path -LiteralPath $log) {
    Copy-Item -LiteralPath $log -Destination "$Out\heyo.log"
    Write-TestLog "copied $log ($((Get-Item -LiteralPath $log).Length) bytes)"
  } else {
    Write-TestLog "no $log"
  }
  Save-TestListing $script:installed 'directory-after-running.txt'
}

Invoke-TestStep 'end Heyo' {
  Stop-TestProcess $script:asker 'the heyo.exe that asked for the settings'
  Stop-TestProcess $script:heyo 'Heyo'
  Start-Sleep 3
  Save-TestTasks 'tasks-ended.txt'
}

Invoke-TestStep 'uninstall' {
  $uninstaller = Join-Path $script:installed 'uninstall.exe'
  if (-not (Test-Path -LiteralPath $uninstaller)) { throw "no $uninstaller" }
  $run = Start-TestProgram $uninstaller @('/S')
  if ($run.WaitForExit(120000)) {
    Write-TestLog "the uninstaller ended with $($run.ExitCode)"
  } else {
    Write-TestLog "the uninstaller, process $($run.Id), has NOT ended after two minutes; going on"
  }
  # It goes on in a copy of itself after it has ended: wait for the program to go.
  $program = Join-Path $script:installed 'heyo.exe'
  $waited = 0
  while ($waited -lt 60 -and (Test-Path -LiteralPath $program)) { Start-Sleep 1; $waited++ }
  Start-Sleep 3
  Write-TestLog ('after the uninstaller and ' + $waited + ' s: heyo.exe ' + (Get-TestPresence $program) + ', the Start menu shortcut ' + (Get-TestPresence $shortcut) + ', the desktop shortcut ' + (Get-TestPresence $onDesktop) + ', registered: ' + (Get-TestRegisteredText))
  Save-TestListing $script:installed 'directory-uninstalled.txt'
}
Write-TestLog 'finished'
