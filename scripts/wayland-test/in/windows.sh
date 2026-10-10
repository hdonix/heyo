#!/bin/sh
# Inside the test session: lists the windows GNOME Shell knows, one per
# line: title | class | x,y WxH | focused. (Harness only; see screenshot.sh.)
gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval \
  "JSON.stringify(global.get_window_actors().map(a => { const w = a.meta_window; const r = w.get_frame_rect(); return [w.get_title(), w.get_wm_class(), r.x + ',' + r.y + ' ' + r.width + 'x' + r.height, w.has_focus() ? 'focused' : ''].join(' | '); }))" \
  | python3 -c "
import sys, re, json
out = sys.stdin.read().strip()
m = re.match(r\"\((true|false), '(.*)'\)\$\", out, re.S)
if not m or m.group(1) != 'true': sys.exit('Eval failed: ' + out)
text = m.group(2).encode().decode('unicode_escape').encode('latin-1').decode('utf-8')
print('\n'.join(json.loads(json.loads(text) if text.startswith('\"') else text)))
"
