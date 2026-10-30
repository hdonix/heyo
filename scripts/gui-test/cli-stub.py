#!/usr/bin/env python3
"""cli-stub.py: Claude Code and Codex for the sandbox, by the name it is
run as (`claude` or `codex`, a link to this file on the sandbox's PATH).

It writes what the real tool writes, in its shape (the stream-json events
of `claude -p`, the JSON lines of `codex exec --json`; the fixtures in
src-tauri/src/models/fixtures/ are their real output), and answers as a
model would: text to work on (`<text>…</text>`) comes back cleaned up and
marked with the tool's name; anything else is answered with a sentence
about launchers, streamed a word at a time by Claude Code and whole by
Codex. Claude Code then goes on for seconds, as the real one does while it
summarizes the turn, which Heyo is not to wait for.

Every call is written down in $CLI_STUB_LOG/<tool>-<n>.json: the
arguments and what came on standard input.
"""
import json
import os
import re
import sys
import time

tool = os.path.basename(sys.argv[0])
args = sys.argv[1:]
if "--version" in args:
    print("2.1.302 (Claude Code)" if tool == "claude" else "codex-cli 0.166.0")
    sys.exit(0)

given = sys.stdin.read()
log = os.environ.get("CLI_STUB_LOG")
if log:
    os.makedirs(log, exist_ok=True)
    number = len([name for name in os.listdir(log) if name.startswith(tool)])
    with open(os.path.join(log, f"{tool}-{number}.json"), "w") as file:
        json.dump({"args": args, "stdin": given, "cwd": os.getcwd(), "cwd_empty": not os.listdir(".")}, file, ensure_ascii=False, indent=1)

name = "Claude Code" if tool == "claude" else "Codex"
if tool == "claude":
    message = json.loads(given.splitlines()[0])["message"]["content"]
    asked = next(block["text"] for block in message if block["type"] == "text")
else:
    asked = given

material = re.fullmatch(r"<text>\n(.*)\n</text>", asked, re.S)
if material:
    text = re.sub(r"\b(um|uh|you know),?\s*", "", material.group(1), flags=re.I).strip()
    text = text[:1].upper() + text[1:]
    answer = f"{text} [{name}]"
else:
    turns = asked.count("<user>")
    answer = f"{name} here (a stub). A launcher opens applications and runs commands from one search field." + (f" This is my answer to message {turns + 1}." if turns else "")


def say(event):
    print(json.dumps(event, ensure_ascii=False), flush=True)


if tool == "claude":
    say({"type": "system", "subtype": "init", "tools": [], "mcp_servers": [], "model": "claude-stub"})
    say({"type": "stream_event", "event": {"type": "message_start", "message": {"model": "claude-stub", "content": []}}})
    say({"type": "stream_event", "event": {"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}})
    for word in re.findall(r"\S+\s*", answer):
        time.sleep(0.08)
        say({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": word}}})
    say({"type": "assistant", "message": {"content": [{"type": "text", "text": answer}]}})
    say({"type": "stream_event", "event": {"type": "content_block_stop", "index": 0}})
    say({"type": "stream_event", "event": {"type": "message_delta", "delta": {"stop_reason": "end_turn"}}})
    say({"type": "stream_event", "event": {"type": "message_stop"}})
    # The summary of the turn, which the real one takes seconds over.
    time.sleep(5)
    say({"type": "result", "subtype": "success", "is_error": False, "result": answer, "stop_reason": "end_turn"})
else:
    say({"type": "thread.started", "thread_id": "stub"})
    say({"type": "turn.started"})
    time.sleep(1.5)
    say({"type": "item.completed", "item": {"id": "item_0", "type": "agent_message", "text": answer}})
    say({"type": "turn.completed", "usage": {"input_tokens": 10, "output_tokens": 10}})
