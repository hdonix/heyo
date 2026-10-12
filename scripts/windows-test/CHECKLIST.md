# Trying the Windows build by hand

Everything below was written on Linux and has only been compiled for
Windows, never run. Each item says what to do in the test machine and what
should happen. Note what fails, with the end of the log.

    scripts/windows-test/build.sh          # on the host
    scripts/windows-test/run.sh start      # on the host; then, inside:
    \\host.lan\Data\scripts\windows-test\in\update.bat
    \\host.lan\Data\scripts\windows-test\in\log.bat      # the log's last lines

The log is `%LOCALAPPDATA%\heyo\heyo.log`. Alt+Space shows Heyo.

## Applications (Start menu shortcuts, icons, Store applications)

- [ ] Open Heyo and type the name of a desktop program (Notepad, Paint,
      a browser). It is listed, once, and Enter starts it.
- [ ] `%LOCALAPPDATA%\heyo\icons` fills with PNG files, 64×64. Open a few:
      the icon is whole, its edges smooth, the background transparent (no
      black or white square), no shortcut arrow.
- [ ] The palette shows those icons. (It will not until the webview accepts
      Windows paths as files: `src/raycast/Image.tsx` only takes paths
      starting with `/`. If rows show no icon but the PNG files are right,
      that is why.)
- [ ] Within a few seconds of the first start, `%LOCALAPPDATA%\heyo\store-apps.json`
      appears and Calculator, Terminal, Settings and other Store
      applications become searchable. Enter starts them. Each has an icon
      file too (made through the shell; note it if they do not).
- [ ] No console window flashes when Heyo starts (PowerShell runs hidden).
- [ ] Start Heyo a second time: applications are there at once (the list is
      read from the file), and nothing in `icons` is rewritten (same times).
- [ ] Install or remove a program with a Start menu shortcut: within about
      five seconds it appears or goes, and so does its icon file.
- [ ] A shortcut with a non-ASCII name or target (make one: a shortcut to
      a program in a folder called `Zoë`) is listed with its icon.

## Going to an application that is already running

- [ ] Start Notepad, bring another window to the front, open Heyo, choose
      Notepad: its existing window comes to the front; no second Notepad.
      The log says `Notepad: 1 window(s) … going to 0x…`.
- [ ] With two windows of one program (two Explorer or browser windows),
      choosing it again while one is in front goes to the other.
- [ ] A program started through an updater (Slack, Discord, Teams classic:
      shortcut target `Update.exe --processStart`): choosing it goes to its
      window rather than starting it again.
- [ ] "Command Prompt" or "PowerShell" always start anew (their windows
      belong to the terminal, by design). So do Store applications (their
      windows belong to a shared host; not handled yet).

## Screen capture

- [ ] Run "Capture Screen" (or the text-recognition command). The overlay
      shows the screen as it was, sharp, not scaled, not offset.
- [ ] Escape closes the overlay without clicking first. A selection can be
      made and saved; the saved picture has the colours right (red is red:
      not swapped with blue) and is not upside down.
- [ ] With display scaling at 150 % (Settings → Display): the same, and
      the capture covers the whole monitor, not its top-left two thirds.
- [ ] With two monitors, the second to the LEFT of the primary (negative
      coordinates; in the VM add a display, or test on real hardware):
      with the pointer on each monitor in turn, the capture is of that
      monitor.
- [ ] Two monitors with different scaling: each capture is still whole
      and sharp.
- [ ] While a UAC prompt or the lock screen is up, the capture fails with
      a message rather than showing black. (Expected to fail cleanly.)
- [ ] Note how long the overlay takes to show the frozen screen, and
      whether the live desktop shows through first: nothing paints the
      capture natively on Windows yet, the webview loads it from a file.

## Selected text

Bind a hotkey to an extension command that uses the selected text, or
use "Selected Text Actions". Each case twice: by hotkey with the
application in front, and from the palette (Heyo in front).

- [ ] Notepad, a browser page, a browser text field, Word or WordPad, VS
      Code, Windows Terminal: the selected text arrives; with nothing
      selected, nothing arrives. The clipboard is unchanged afterwards
      (copy something distinctive first and paste it after).
- [ ] An application that is not accessible (try an old Win32 or Java
      program; by hotkey only): the text arrives by way of Ctrl+C. The
      clipboard afterwards holds what it held before, in every format (try
      with an image on it, and with files copied in Explorer: paste them
      afterwards). Heyo's clipboard history shows neither the selection
      nor a second copy of the old contents; nor does Win+V.
- [ ] In a terminal running `ping -t localhost`, with nothing selected,
      ask for the selected text by hotkey: the ping keeps running (no
      Ctrl+C was sent).
- [ ] With a very large clipboard (copy a big image), the Ctrl+C way is
      not slow enough to notice; note it if it is.
- [ ] An application that is busy (a browser loading a huge page): Heyo
      answers "nothing" within a second and does not hang.

## Hold-to-talk key

Settings → Dictation → hold key. (Recording itself does not work on
Windows yet; watch the log for `push-to-talk:` lines.)

- [ ] The setting offers the keys (Right Alt, Left Alt, Right Ctrl, …).
- [ ] Hold Right Ctrl alone for a second: the log says `recording`, and
      on release `released; finishing`.
- [ ] Tap it: nothing is logged.
- [ ] Right Ctrl then C quickly (a shortcut): no `recording`.
- [ ] Hold it, and press another key a moment after `recording`:
      `another key; cancelled`.
- [ ] Hold Shift, then Right Ctrl as well: no `recording`.
- [ ] Typing feels the same with the key set as with none: no lag, no
      lost or doubled characters, also under load (run a build). Key
      repeat while holding a letter is smooth.
- [ ] The watched key still does its ordinary job (Right Ctrl+C copies;
      Caps Lock toggles).
- [ ] On a layout where Right Alt is AltGr (German, say): Right Alt can be
      the key (the Ctrl that Windows invents before it does not count as
      "another key"), and AltGr+Q still types @.
- [ ] Set the key to "none": the log says `push-to-talk key: none`. Exit
      Heyo (or kill it in Task Manager) with a key set: the keyboard
      behaves normally at once.
- [ ] Hold the key, press Ctrl+Alt+Del, release it there, come back: the
      log says `key no longer held; finishing` within a second.
- [ ] Dictation started by its ordinary hotkey and held: releasing the
      hotkey after a moment finishes it (this needs Heyo to see which keys
      are down, which it now can on Windows).

## Clipboard history

- [ ] Copy text and an image in other programs: both appear in the history.
- [ ] Copy a password in KeePassXC, 1Password or Bitwarden: it does not.
- [ ] In PowerShell, put text on the clipboard that asks to stay out of
      the history, and check that it does (it should not appear):
      `Add-Type -A System.Windows.Forms; $d = New-Object Windows.Forms.DataObject; $d.SetText('secret-1'); $d.SetData('ExcludeClipboardContentFromMonitorProcessing', [IO.MemoryStream]::new([byte[]](1,0,0,0))); [Windows.Forms.Clipboard]::SetDataObject($d, $true)`
      and the same with `'CanIncludeInClipboardHistory'` and bytes `(0,0,0,0)`.
- [ ] Paste an entry from the history into Notepad: it is pasted, and the
      history does not gain a duplicate from Heyo's own write.
- [ ] An extension copying with `concealed: true` leaves no entry.

## Window layouts

- [ ] Left Half, Right Half, the quarters and thirds: the window's visible
      edges meet the screen's edges and each other; no gap of a few pixels
      at the sides or bottom, none hidden under the taskbar.
- [ ] The same on a maximized window: it is restored, then placed.
- [ ] Maximize, Center, Almost Maximize.
- [ ] Move to Next Display with two monitors: the window lands wholly on
      the other one, in the same relative place and proportion; again, and
      it comes back where it was. A maximized window arrives maximized.
      With one monitor: "There is only one display."
- [ ] With the monitors at different scaling, the same.
- [ ] An extension that sets a window's bounds (Raycast's
      `WindowManagement.setWindowBounds`) to what it just read from
      `getWindows`: the window does not move or change size.
- [ ] A window of a program run as administrator: the layout fails with a
      message ("did not let the window be moved"), not silently.

## Compile warnings

- [ ] None of this needs the machine: the build for Windows
      (`scripts/package/build.sh aarch64-pc-windows-msvc`) reports none in
      Heyo's own code.
