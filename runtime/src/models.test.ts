// The model seam: the settings pick the adapter, the OpenAI-compatible one
// speaks to a server as vLLM or Ollama would answer, and every failure comes
// back in words for the user.
import http from "node:http";
import type { AddressInfo } from "node:net";

import Anthropic from "@anthropic-ai/sdk";
import { afterAll, beforeAll, expect, test } from "vitest";

import { ModelError } from "./model";
import { describeModelError, modelFrom } from "./models";

const settings = { apiKey: "", localUrl: "", localModel: "" };

test("the settings name the adapter, and what it can be given", () => {
  const claude = modelFrom({ ...settings, provider: "claude" });
  const local = modelFrom({ ...settings, provider: "local", localUrl: "http://127.0.0.1:9/v1" });
  expect([claude.provider, claude.sees.images]).toEqual(["claude", true]);
  expect([local.provider, local.sees.images]).toEqual(["local", false]);
});

// A server that lists one model and streams its answer in pieces, as an
// OpenAI-compatible one does; it keeps what it was asked.
let server: http.Server;
let url = "";
const asked: any[] = [];
beforeAll(async () => {
  server = http.createServer((request, response) => {
    if (request.url === "/v1/models") return response.end(JSON.stringify({ data: [{ id: "tiny" }] }));
    let body = "";
    request.on("data", (chunk) => (body += chunk));
    request.on("end", () => {
      asked.push(JSON.parse(body));
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const piece of ["<think>hmm</think>", "Hello", ", world"]) {
        response.write(`data: ${JSON.stringify({ choices: [{ delta: { content: piece } }] })}\n\n`);
      }
      response.end("data: [DONE]\n\n");
    });
  });
  await new Promise<void>((listening) => server.listen(0, "127.0.0.1", listening));
  url = `http://127.0.0.1:${(server.address() as AddressInfo).port}/v1`;
});
afterAll(() => server.close());

test("a local model is asked for text to work on as material, and its reasoning is left out", async () => {
  const seen: string[] = [];
  const answer = await modelFrom({ ...settings, provider: "local", localUrl: url }).answer({ system: "Translate.", prompt: "Bonjour", onText: (text) => seen.push(text) });
  expect(answer).toBe("Hello, world");
  expect(seen.at(-1)).toBe("Hello, world");
  const [question] = asked;
  expect(question.model).toBe("tiny");
  expect(question.temperature).toBe(0);
  expect(question.messages.at(-1).content).toContain("<text>\nBonjour\n</text>");
});

test("a model that sees no images refuses them, in words for the user", async () => {
  const asking = modelFrom({ ...settings, provider: "local", localUrl: url }).answer({ system: "", prompt: "What is this?", images: [{ mediaType: "image/png", data: "" }] });
  await expect(asking).rejects.toBeInstanceOf(ModelError);
  expect(describeModelError(await asking.catch((error) => error))).toBe("The local model is not given images.");
});

test("an unreachable server and Claude's own errors are worded for the user", async () => {
  const unreachable = await modelFrom({ ...settings, provider: "local", localUrl: "http://127.0.0.1:9/v1", localModel: "tiny" })
    .answer({ system: "", prompt: "x" })
    .catch((error) => error);
  expect(describeModelError(unreachable)).toMatch(/Could not reach the local model at http:\/\/127\.0\.0\.1:9/);
  expect(describeModelError(new Anthropic.RateLimitError(429, undefined, "slow down", new Headers()))).toMatch(/rate limit/);
  expect(describeModelError(new Error("something else"))).toBe("something else");
});
