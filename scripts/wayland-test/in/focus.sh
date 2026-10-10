#!/bin/sh
# Inside the test session: the focused window's title, sampled every 50 ms
# for N seconds (default 3), printed whenever it changes.
exec gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "
  globalThis._focusLog = [];
  const started = GLib.get_monotonic_time();
  GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
    const t = Math.round((GLib.get_monotonic_time() - started) / 1000);
    const title = global.display.focus_window?.get_title() ?? '(none)';
    const log = globalThis._focusLog;
    if (!log.length || log[log.length - 1][1] !== title) log.push([t, title]);
    return t < ${1:-3} * 1000;
  }); 'sampling'"
