#!/bin/sh
# Makes the archive extensions.gnome.org takes for review:
#
#   gnome-extension/package.sh      writes gnome-extension/build/heyo@heyo.app.shell-extension.zip
#
# The archive holds the extension's two files and nothing else, at its top
# level, as the site expects. See README.md here for what a reviewer is told.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
exec python3 - "$HERE" <<'PY'
import json, os, sys, zipfile

here = sys.argv[1]
uuid = "heyo@heyo.app"
source = os.path.join(here, uuid)
metadata = json.load(open(os.path.join(source, "metadata.json")))

problems = []
for key in ("uuid", "name", "description", "shell-version", "url"):
    if not metadata.get(key):
        problems.append(f"metadata.json has no \"{key}\" (the site requires it)")
if metadata.get("uuid") != uuid:
    problems.append(f"metadata.json's uuid is not {uuid}")
# The site reviews source: nothing compiled, nothing but text.
files = sorted(os.listdir(source))
for name in files:
    data = open(os.path.join(source, name), "rb").read()
    if b"\0" in data or not name.endswith((".js", ".json", ".css")):
        problems.append(f"{name} is not a source file")
if files != ["extension.js", "metadata.json"]:
    problems.append(f"unexpected files: {files}")
if problems:
    sys.exit("not packaged:\n  " + "\n  ".join(problems))

os.makedirs(os.path.join(here, "build"), exist_ok=True)
out = os.path.join(here, "build", f"{uuid}.shell-extension.zip")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
    for name in files:
        archive.write(os.path.join(source, name), name)
print(out)
PY
