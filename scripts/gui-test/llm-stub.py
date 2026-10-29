#!/usr/bin/env python3
"""llm-stub.py PORT [LOG]: a local model that is not one. An OpenAI-compatible
server on 127.0.0.1 whose one model answers every question with canned
text, streamed a word at a time so that streaming can be watched. The
answer repeats the question, so the test can tell which was answered.

A question with "slowly" in it is answered at a word every half second, a
long answer, so that it can be called off in the middle.

With LOG, each answer is written down there, a line at a time: `asked after N
messages: QUESTION` when it begins, then `answered` when all of it went out, or
`closed after N words` when whoever asked closed the connection first."""
import json, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ANSWER = "You asked: “{question}”. Here is a **canned** answer from the stub, with a list:\n\n- one\n- two\n\n```sh\necho done\n```\n"
LOG = sys.argv[2] if len(sys.argv) > 2 else None


def note(line):
    if LOG:
        with open(LOG, "a") as log:
            log.write(line.replace("\n", " ") + "\n")


class Stub(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            return self.reply({"data": [{"id": "stub-model"}]})
        self.send_error(404)

    def do_POST(self):
        if not self.path.rstrip("/").endswith("/chat/completions"):
            return self.send_error(404)
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", 0)) or 0) or b"{}")
        asked = next((m["content"] for m in reversed(body.get("messages", [])) if m.get("role") == "user"), "")
        slowly = "slowly" in asked
        pieces = ANSWER.format(question=asked).split(" ") * (8 if slowly else 1)
        note(f"asked after {len(body.get('messages', []))} messages: {asked[-200:]}")
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        try:
            for count, piece in enumerate(pieces):
                self.wfile.write(f"data: {json.dumps({'choices': [{'delta': {'content': piece + ' '}}]})}\n\n".encode())
                self.wfile.flush()
                time.sleep(0.5 if slowly else 0.12)
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            note(f"closed after {count} words")
            return
        note("answered")

    def reply(self, value):
        data = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Stub).serve_forever()
