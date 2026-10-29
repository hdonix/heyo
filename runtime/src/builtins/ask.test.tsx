// Ask AI, rendered as the runtime renders it: the question becomes a kept
// conversation, and AI Chat takes it up from there.
import { createElement } from "react";
import { afterAll, expect, test, vi } from "vitest";

import { answers, requests, reset, sent } from "../testing";

const model = vi.hoisted(() => ({ calls: [] as { prompt: string; onText(text: string): void; finish(answer: string): void }[] }));
vi.mock("../ai", () => ({
  askModel: (options: { prompt: string; onText(text: string): void }) => new Promise<string>((finish) => model.calls.push({ ...options, finish })),
}));

import { Navigator } from "../api/navigation";
import { foreground, runAs, scoped, type Current } from "../context";
import { AI } from "../heyo/ai";
import { dispatch, mount, visibleChildren, type Container, type HostNode } from "../renderer";
import { close } from "../session";
import { AskAi } from "./ask";
import { chats } from "./chat";
import { builtinExtension } from "./index";

const askedOfApi = vi.spyOn(AI, "ask");
const command: Current = { extension: builtinExtension, command: "ask-ai", launchType: "userInitiated" };

afterAll(() => close());

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

test("a question asked is a conversation kept, which AI Chat continues", async () => {
  // `AI.ask` asks the host one thing, which model answers.
  answers["ext.ai.provider"] = () => ({ kind: "claude", model: "", local: false });
  const errors: unknown[] = [];
  const container: Container = { children: [], onCommit() {} };
  const root = createElement(Navigator, { root: createElement(AskAi, { fallbackText: " Why is the sky blue? " }), onPopRoot() {}, expose() {} });
  // As the session mounts it: the command in the palette.
  foreground.value = command;
  const unmount = runAs(command, () => mount(scoped(command, root), container, (e) => errors.push(e)));
  await settle();
  const all = () => {
    const out: HostNode[] = [];
    const visit = (node: HostNode) => {
      out.push(node);
      visibleChildren(node).forEach(visit);
    };
    container.children.forEach(visit);
    return out;
  };
  const titles = () => all().filter((n) => n.type === "Action").map((n) => n.props.title);
  const detail = () => all().find((n) => n.type === "Detail")!;

  expect(errors).toEqual([]);
  // Asked once, through `@heyo/api` as an extension asks, and kept at once.
  expect(askedOfApi).toHaveBeenCalledTimes(1);
  expect(askedOfApi.mock.calls[0][0]).toBe("Why is the sky blue?");
  expect(askedOfApi.mock.calls[0][1]).toMatchObject({ messages: [], system: expect.stringContaining("quick-answer assistant") });
  expect(requests).toEqual([{ method: "ext.ai.provider", extension: "@heyo", dir: "" }]);
  expect(model.calls.map((c) => c.prompt)).toEqual(["Why is the sky blue?"]);
  const [kept] = chats().store.list();
  expect(kept).toMatchObject({ title: "Why is the sky blue?", messages: [{ role: "user", content: "Why is the sky blue?" }] });
  expect(detail().props).toMatchObject({ isLoading: true, markdown: "> Why is the sky blue?\n\n…", untrusted: true });
  expect(titles()).toEqual(["Stop Answering"]);

  model.calls[0].onText("Rayleigh");
  await settle(200);
  expect(detail().props.markdown).toBe("> Why is the sky blue?\n\nRayleigh");

  model.calls[0].finish("Rayleigh scattering.");
  await settle(100);
  expect(detail().props).toMatchObject({ isLoading: false, markdown: "> Why is the sky blue?\n\nRayleigh scattering." });
  expect(titles()).toEqual(["Continue in AI Chat", "Copy Answer", "Paste Answer", "Try Again"]);
  expect(chats().store.get(kept.id)?.messages).toHaveLength(2);

  // Continuing starts AI Chat, on this conversation.
  reset();
  runAs(command, () => dispatch(all().find((n) => n.props.title === "Continue in AI Chat")!.id, "onAction", []));
  await settle(100);
  unmount();
  expect(sent.find((m) => m.type === "launched")).toMatchObject({ title: "AI Chat" });
  const shown = sent.filter((m) => m.type === "render" && m.view).at(-1).view;
  // On top of the list of conversations.
  expect(shown.depth).toBe(2);
  const list = shown.nodes.find((n: { type: string }) => n.type === "List");
  expect(list.props).toMatchObject({ navigationTitle: "Why is the sky blue?", isShowingDetail: true });
  expect(shown.nodes.find((n: { type: string }) => n.type === "List.Item.Detail").props.markdown).toBe("> Why is the sky blue?\n\nRayleigh scattering.");
  expect(errors).toEqual([]);
});
