"""seed.py FILE [name=value…]: writes Heyo's settings for a test to start from.

Values are JSON where they parse as JSON, else text. The vocabulary always
has Chinese in it: the three characters an English Windows had no glyph for,
and a sentence of ordinary Chinese.
"""
import json
import sys

settings = {
    "hideOnBlur": False,
    "appearance": "light",
    # Whatever language the desktop these run on speaks: the scripts find
    # windows and controls by their English names.
    "language": "en",
    "dictationVocabulary": [
        "TypeScript",
        "Kubernetes",
        "发 这 议",
        "杰森 -> JSON",
        "这是一段普通的中文，用来看设置窗口里的汉字：开发、会议、这里、设置、语音输入。",
    ],
}
for assignment in sys.argv[2:]:
    name, _, value = assignment.partition("=")
    try:
        settings[name] = json.loads(value)
    except ValueError:
        settings[name] = value
with open(sys.argv[1], "w") as file:
    json.dump(settings, file, ensure_ascii=False, indent=2)
