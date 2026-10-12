# Tries Heyo's Slint settings window on Windows, as a person would with the
# pointer, and writes down what was seen (test.bat runs this).
#
#   1. Skia on the CPU, the renderer it is drawn with: every page built, the
#      theme changed from its select, and whether that reached Heyo's stored
#      settings; closed and asked for again.
#   2. Slint's own renderer, for the pages again.
#   3. A panic on Slint's thread, asked for: Heyo must still be running, the
#      Slint window gone, and the settings asked for again must be the web ones.
#
# Heyo keeps its files under .\home-<run> here, not where the machine's own
# Heyo has them. It is started by this script and ended by the number it
# was given when started.
#
# The window is found among the windows of that process (every one of them
# is written to test.log each time, so that a window this does not know is
# at least named there). The settings window is 860x640 with a margin for
# its shadow, and cannot be made smaller: on a smaller screen it is moved so
# that its top left corner is the screen's, and what is beyond the screen's
# right and lower edges is neither seen nor clicked (the log says what).
#
# Whatever goes wrong is written to test.log, and the next run is tried.
param([string]$Out)
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
function Note-Line($text) { Add-Content -Path "$Out\test.log" -Value ("{0:HH:mm:ss} {1}" -f (Get-Date), $text) }

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -ReferencedAssemblies System.Drawing @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Native {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  public delegate bool EnumProc(IntPtr window, IntPtr data);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr window, StringBuilder name, int count);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, int data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
  public static List<IntPtr> WindowsOf(uint process) {
    List<IntPtr> found = new List<IntPtr>();
    EnumWindows(delegate(IntPtr window, IntPtr data) {
      uint owner;
      GetWindowThreadProcessId(window, out owner);
      if (owner == process) found.Add(window);
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static string Title(IntPtr window) { StringBuilder text = new StringBuilder(512); GetWindowTextW(window, text, 512); return text.ToString(); }
  public static string Class(IntPtr window) { StringBuilder text = new StringBuilder(256); GetClassNameW(window, text, 256); return text.ToString(); }
}
"@
[Native]::SetProcessDPIAware() | Out-Null

function Get-Box($window) { $rect = New-Object Native+RECT; [Native]::GetWindowRect($window, [ref]$rect) | Out-Null; return $rect }
function Describe-Window($window) {
  if ($window -eq [IntPtr]::Zero) { return "none" }
  $rect = Get-Box $window
  return "'$([Native]::Title($window))' class '$([Native]::Class($window))' at $($rect.Left),$($rect.Top) size $($rect.Right - $rect.Left)x$($rect.Bottom - $rect.Top) $([Native]::GetDpiForWindow($window)) dpi$(if ([Native]::IsWindowVisible($window)) { '' } else { ' (hidden)' })"
}
# Every window of the process, written down; answers the settings window
# that is on screen, if one is.
function Find-Settings($heyo, $when) {
  $found = [IntPtr]::Zero
  foreach ($window in [Native]::WindowsOf([uint32]$heyo.Id)) {
    if ($when) { Note-Line "  ${when}: window $(Describe-Window $window)" }
    if ([Native]::IsWindowVisible($window) -and [Native]::Title($window) -eq "Heyo Settings") { $found = $window }
  }
  return $found
}
function Await-Settings($heyo) {
  for ($i = 0; $i -lt 100; $i++) {
    $window = Find-Settings $heyo $null
    if ($window -ne [IntPtr]::Zero) { return $window }
    Start-Sleep -Milliseconds 100
  }
  return [IntPtr]::Zero
}

# The margin the window keeps around its panel for a shadow: none when the
# renderer draws none.
function Get-Margin($window) { $rect = Get-Box $window; $scale = [Native]::GetDpiForWindow($window) / 96.0; return [int](($rect.Right - $rect.Left - 860 * $scale) / 2) }
# Moves the window so that its panel begins where the screen does.
function Move-ToCorner($window) {
  if ($window -eq [IntPtr]::Zero) { return }
  $margin = Get-Margin $window
  [Native]::SetWindowPos($window, [IntPtr]::Zero, -$margin, -$margin, 0, 0, 0x0005) | Out-Null
  [Native]::SetForegroundWindow($window) | Out-Null
  Start-Sleep -Milliseconds 500
}

# The screen. (PrintWindow was tried for the window whole: it gives only
# the part that is on the screen.)
function Save-Picture($name, $window) {
  $screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $picture = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
  $graphics = [System.Drawing.Graphics]::FromImage($picture)
  $graphics.CopyFromScreen($screen.Left, $screen.Top, 0, 0, $picture.Size)
  $picture.Save("$Out\$name.png")
  $graphics.Dispose(); $picture.Dispose()
  Note-Line "picture $name.png$(if (-not $window -or $window -eq [IntPtr]::Zero) { ' (no settings window was found)' })"
}

# A place in the window's panel, in its own logical pixels, as a place on screen.
function Get-Spot($window, $x, $y) {
  $rect = Get-Box $window; $scale = [Native]::GetDpiForWindow($window) / 96.0; $margin = Get-Margin $window
  return @([int]($rect.Left + $margin + $x * $scale), [int]($rect.Top + $margin + $y * $scale))
}
function Point-Mouse($window, $x, $y) { $to = Get-Spot $window $x $y; [Native]::SetCursorPos($to[0], $to[1]) | Out-Null; Start-Sleep -Milliseconds 150 }
function Press-Mouse($window, $x, $y) {
  Point-Mouse $window $x $y
  [Native]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [Native]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
}
function Turn-Wheel($notches) { for ($i = 0; $i -lt $notches; $i++) { [Native]::mouse_event(0x0800, 0, 0, -120, [UIntPtr]::Zero); Start-Sleep -Milliseconds 30 } }
# The entry of the sidebar at a row: the pages, a gap, the extensions.
function Press-Entry($window, $row) {
  $gap = 0; if ($row -ge 6) { $gap = 14 }
  Press-Mouse $window 120 (8 + 58 + $row * 34 + 17 + $gap)
}

# Starts Heyo with a home of its own and the variables given; answers the process.
function Start-Heyo($name, $variables) {
  $dir = "$here\home-$name"
  if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
  New-Item -ItemType Directory -Force "$dir\config\heyo" | Out-Null
  Copy-Item "$here\settings.json" "$dir\config\heyo\settings.json"
  $env:XDG_CONFIG_HOME = "$dir\config"; $env:XDG_DATA_HOME = "$dir\data"; $env:XDG_CACHE_HOME = "$dir\cache"
  $env:HEYO_SETTINGS = "slint"
  $env:HEYO_SETTINGS_RENDERER = $null; $env:HEYO_SLINT_PANIC_AFTER = $null
  foreach ($variable in $variables.Keys) { Set-Item -Path "Env:$variable" -Value $variables[$variable] }
  $heyo = Start-Process -FilePath "$here\heyo.exe" -ArgumentList "--hidden" -PassThru
  Note-Line "${name}: started Heyo, process $($heyo.Id), with $(($variables.Keys | ForEach-Object { "$_=$($variables[$_])" }) -join ' ')"
  Start-Sleep 12
  return $heyo
}
function Open-Settings() { Start-Process -FilePath "$here\heyo.exe" -ArgumentList "--open", "heyo:settings" -Wait }
function Test-Alive($heyo) { return $null -ne (Get-Process -Id $heyo.Id -ErrorAction SilentlyContinue) }
function Get-State($heyo) { if (Test-Alive $heyo) { return "still running" } else { return "NOT running any more" } }
function Stop-Heyo($name, $heyo) {
  Note-Line "${name}: Heyo is $(Get-State $heyo), using $([int]((Get-Process -Id $heyo.Id -ErrorAction SilentlyContinue).WorkingSet64 / 1MB)) MB"
  # By its number, with what it started itself.
  cmd /c "taskkill /PID $($heyo.Id) /T /F >nul 2>&1"
  Start-Sleep 2
  Get-ChildItem -Recurse -Filter *.log "$here\home-$name" -ErrorAction SilentlyContinue | ForEach-Object { Copy-Item $_.FullName "$Out\$name-$($_.Name)" }
}

# The pages built in Slint, each as a picture. Without a window to click
# in, the screen is pictured all the same.
function Take-Tour($name, $window) {
  Save-Picture "$name-general" $window
  if ($window -eq [IntPtr]::Zero) { return }
  Press-Entry $window 2; Start-Sleep 3; Save-Picture "$name-dictation" $window
  Point-Mouse $window 528 368; Turn-Wheel 12; Start-Sleep 2; Save-Picture "$name-dictation-vocabulary" $window
  Press-Entry $window 6; Start-Sleep 4; Save-Picture "$name-applications" $window
  Point-Mouse $window 528 368; Turn-Wheel 15; Start-Sleep 2; Save-Picture "$name-applications-scrolled" $window
  Press-Entry $window 5; Start-Sleep 2; Save-Picture "$name-about" $window
  Press-Entry $window 0; Start-Sleep 2
}

$screen = [System.Windows.Forms.SystemInformation]::VirtualScreen
Note-Line "screen $screen, $((Get-CimInstance Win32_OperatingSystem).Caption) $env:PROCESSOR_ARCHITECTURE"
if ($screen.Width -lt 924 -or $screen.Height -lt 704) { Note-Line "the settings window (924x704 with its margin, 860x640 without) does not fit this screen: it is moved to the corner, and its right and lower edges are beyond the screen. Not reachable at 800x600: the switches (796 to 832 of the panel's 860) and the close button; the theme's select is clicked instead, at 748" }
$heyo = $null
function Run-Guarded($name, $work) {
  try { & $work } catch {
    Note-Line "${name}: STOPPED by an error: $($_.Exception.Message) at line $($_.InvocationInfo.ScriptLineNumber): $($_.InvocationInfo.Line.Trim())"
    Save-Picture "$name-stopped" $null
  }
  if ($script:heyo) { try { Stop-Heyo $name $script:heyo } catch { Note-Line "${name}: could not end Heyo: $($_.Exception.Message)" }; $script:heyo = $null }
}

# --- 1. Skia on the CPU ---
Run-Guarded "skia" {
$script:heyo = Start-Heyo "skia" @{}
$asked = Get-Date
Open-Settings
$window = Await-Settings $script:heyo
Note-Line "skia: settings window $(Describe-Window $window), $([int]((Get-Date) - $asked).TotalMilliseconds) ms after it was asked for (with the starting of a second heyo.exe in it)"
Find-Settings $script:heyo "skia" | Out-Null
Start-Sleep 2
Move-ToCorner $window
Take-Tour "skia" $window
if ($window -ne [IntPtr]::Zero) {
  # The theme, from its select: open it, choose Dark.
  Press-Mouse $window 748 233; Start-Sleep 2; Save-Picture "skia-theme-select" $window
  Press-Mouse $window 743 327; Start-Sleep 3; Save-Picture "skia-general-dark" $window
  Note-Line "skia: stored settings after choosing Dark: $((Get-Content "$here\home-skia\config\heyo\settings.json" -Raw) -replace '\s+', ' ')"
  Press-Entry $window 2; Start-Sleep 2; Point-Mouse $window 528 368; Turn-Wheel 12; Start-Sleep 2; Save-Picture "skia-dictation-vocabulary-dark" $window
  # Close, and ask again: how long the second time takes.
  [System.Windows.Forms.SendKeys]::SendWait("{ESC}"); Start-Sleep 2
  Note-Line "skia: after Esc, settings window $(Describe-Window (Find-Settings $script:heyo $null))"
  $asked = Get-Date; Open-Settings
  $window = Await-Settings $script:heyo
  Note-Line "skia: asked again, settings window $(Describe-Window $window), $([int]((Get-Date) - $asked).TotalMilliseconds) ms after"
  Start-Sleep 2; Move-ToCorner $window; Save-Picture "skia-again" $window
}
}

# --- 2. Slint's own renderer ---
Run-Guarded "software" {
$script:heyo = Start-Heyo "software" @{ HEYO_SETTINGS_RENDERER = "software" }
Open-Settings
$window = Await-Settings $script:heyo
Note-Line "software: settings window $(Describe-Window $window)"
Find-Settings $script:heyo "software" | Out-Null
Start-Sleep 2
Move-ToCorner $window
Take-Tour "software" $window
}

# --- 3. A panic on Slint's thread ---
Run-Guarded "panic" {
$script:heyo = Start-Heyo "panic" @{ HEYO_SLINT_PANIC_AFTER = "8000" }
Open-Settings
$window = Await-Settings $script:heyo
Note-Line "panic: before it, settings window $(Describe-Window $window)"
Move-ToCorner $window
Save-Picture "panic-1-before" $window
Start-Sleep 10
$window = Find-Settings $script:heyo "panic, after it"
Note-Line "panic: after it, settings window $(Describe-Window $window); Heyo is $(Get-State $script:heyo)"
Save-Picture "panic-2-after" $window
Open-Settings
$window = Await-Settings $script:heyo
Find-Settings $script:heyo "panic, asked again" | Out-Null
Note-Line "panic: asked again, settings window $(Describe-Window $window); Heyo is $(Get-State $script:heyo)"
Start-Sleep 4
Move-ToCorner $window
Save-Picture "panic-3-asked-again" $window
}
Note-Line "finished"
