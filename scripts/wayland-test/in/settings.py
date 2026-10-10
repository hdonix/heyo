#!/usr/bin/env python3
"""Inside the test session: writes Heyo's settings. Arguments are key=value,
values in JSON: settings.py hideOnBlur=false dictationPolish='"off"'"""
import json, os, sys
path = os.path.expanduser("~/.config/heyo/settings.json")
os.makedirs(os.path.dirname(path), exist_ok=True)
settings = json.load(open(path)) if os.path.exists(path) else {}
for pair in sys.argv[1:]:
    key, value = pair.split("=", 1)
    settings[key] = json.loads(value)
json.dump(settings, open(path, "w"), indent=2)
print(settings)
