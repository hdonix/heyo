#!/usr/bin/env python3
"""Inside the test session: runs JavaScript in GNOME Shell and prints what it
evaluates to, as JSON. (Harness only: the Shell refuses this on a real desktop.)

  shell.py 'global.display.get_n_monitors()'

In scope there: global, Main, Meta, Shell, GLib, Gio; others as imports.gi.Clutter.
From Python: `from shell import run`.
"""
import json, subprocess, sys

def run(script):
    """What `script` evaluates to, parsed. It is wrapped so that anything
    JSON can say comes back."""
    done = subprocess.run(
        ["gdbus", "call", "--session", "--dest", "org.gnome.Shell", "--object-path", "/org/gnome/Shell",
         "--method", "org.gnome.Shell.Eval", f"JSON.stringify((() => {{ return {script}; }})() ?? null)"],
        capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"the Shell did not answer: {done.stderr.strip()}")
    out = done.stdout.strip()
    # (true, '"…"'): a GVariant tuple whose string is the JSON, quoted once more by Eval.
    from gi.repository import GLib
    ok, text = GLib.Variant.parse(GLib.VariantType("(bs)"), out, None, None).unpack()
    if not ok:
        raise RuntimeError(f"the Shell could not run it: {text}")
    value = json.loads(text) if text else None
    return json.loads(value) if isinstance(value, str) else value

if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    print(json.dumps(run(sys.argv[1]), indent=2, ensure_ascii=False))
