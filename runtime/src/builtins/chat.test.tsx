// The chat view, rendered as the runtime renders it: what a long
// conversation costs while an answer is being written.
import { createElement } from "react";
import { beforeEach, expect, test, vi } from "vitest";

import { answers, reset } from "../testing";

// The model: answers when the test says so, and fails when its question is withdrawn.
const model = vi.hoisted(() => ({ calls: [] as { prompt: string; history: unknown[]; signal: AbortSignal; onText(text: string): void; finish(answer: string): void }[] }));
vi.mock("../ai", () => ({
  CLAUDE_MODEL: "claude",
  describeModelError: (error: unknown) => String(error),
  askModel: (options: { prompt: string; history: unknown[]; signal: AbortSignal; onText(text: string): void }) =>
    new Promise<string>((finish, fail) => {
      options.signal.addEventListener("abort", () => fail(new Error("aborted")));
      model.calls.push({ ...options, finish });
    }),
}));

import { Navigator } from "../api/navigation";
import { foreground, runAs, scoped, type Current } from "../context";
import { AI } from "../heyo/ai";
import { dispatch, encodeProps, mount, visibleChildren, type Container, type HostNode } from "../renderer";
import { AiChat, begin, Chat, chats } from "./chat";
import { answered, asked, newConversation, type Conversation } from "./conversations";
import { builtinExtension } from "./index";

const askedOfApi = vi.spyOn(AI, "ask");
const command: Current = { extension: builtinExtension, command: "ai-chat", launchType: "userInitiated" };

beforeEach(() => {
  reset();
  // `AI.ask` asks the host one thing, which model answers.
  answers["ext.ai.provider"] = () => ({ kind: "claude", model: "", local: false });
});

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

/// Mounts a view the way a session does, and tells what is on top.
async function show(element: React.ReactElement) {
  const errors: unknown[] = [];
  const container: Container = { children: [], onCommit() {} };
  const root = createElement(Navigator, { root: element, onPopRoot() {}, expose() {} });
  foreground.value = command;
  const unmount = runAs(command, () => mount(scoped(command, root), container, (e) => errors.push(e)));
  await settle();
  const all = () => {
    const out: HostNode[] = [];
    const visit = (node: HostNode) => {
      out.push(node);
      visibleChildren(node).forEach(visit);
    };
    visibleChildren(container.children.filter((c) => c.type === "View").at(-1)!).forEach(visit);
    return out;
  };
  return {
    errors,
    unmount,
    all,
    of: (type: string) => all().filter((n) => n.type === type),
    /// What the session compares to decide which nodes to send again.
    signatures: () => new Map(all().map((n) => [n.id, JSON.stringify([encodeProps(n), visibleChildren(n).map((c) => c.id)])])),
  };
}

const changedBetween = (before: Map<number, string>, after: Map<number, string>) => [...after].filter(([id, signature]) => before.get(id) !== signature).map(([id]) => id);

function long(exchangeCount: number): Conversation {
  let conversation = newConversation(1000);
  for (let i = 0; i < exchangeCount; i++) {
    conversation = answered(asked(conversation, `Question ${i}`, 1000 + i), `Answer ${i}. ${"word ".repeat(200)}`, 1000 + i);
  }
  return conversation;
}

test("while an answer is written, only that answer changes", async () => {
  const conversation = long(40);
  chats().store.save(conversation);
  const view = await show(createElement(Chat, { conversation }));
  expect(view.errors).toEqual([]);
  // The latest exchange is the first row.
  expect(view.of("List.Item")).toHaveLength(40);
  expect(view.of("List.Item")[0].props.title).toBe("Question 39");

  // A reply typed in the search bar, and Enter.
  const list = view.of("List")[0];
  dispatch(list.id, "onSearchTextChange", ["And one more?"]);
  await settle();
  const send = view.of("Action").find((a) => a.props.title === "Send Message")!;
  dispatch(send.id, "onAction", []);
  await settle();

  // Asked through `@heyo/api`: the conversation so far as `messages`, the
  // reply as the prompt; the model gets the turns in order.
  expect(askedOfApi).toHaveBeenCalledTimes(1);
  expect(askedOfApi.mock.calls[0][0]).toBe("And one more?");
  const options = askedOfApi.mock.calls[0][1]!;
  expect(options.messages).toHaveLength(80);
  expect(options.messages![0]).toEqual({ role: "user", content: "Question 0" });
  expect(options.messages![79]).toMatchObject({ role: "assistant", content: expect.stringContaining("Answer 39.") });
  expect(options.system).toMatch(/quick-answer assistant/);
  expect(model.calls).toHaveLength(1);
  expect(model.calls[0].prompt).toBe("And one more?");
  expect(model.calls[0].history).toEqual(options.messages);
  expect(view.of("List.Item")).toHaveLength(41);
  expect(view.of("List.Item")[0].props.title).toBe("And one more?");
  // The question is kept before there is an answer.
  expect(chats().store.get(conversation.id)?.messages).toHaveLength(81);

  model.calls[0].onText("One");
  await settle(200);
  const before = view.signatures();
  model.calls[0].onText("One more thing");
  await settle(200);
  const after = view.signatures();

  const changed = changedBetween(before, after);
  expect(changed).toHaveLength(1);
  const detail = view.all().find((n) => n.id === changed[0])!;
  expect(detail.type).toBe("List.Item.Detail");
  expect(detail.props.markdown).toBe("> And one more?\n\nOne more thing");
  // What goes to the window is the one answer, not the conversation.
  expect(after.get(detail.id)!.length).toBeLessThan(200);

  model.calls[0].finish("One more thing, said in full.");
  await settle(100);
  expect(view.all().find((n) => n.id === detail.id)!.props.markdown).toBe("> And one more?\n\nOne more thing, said in full.");
  expect(chats().store.get(conversation.id)?.messages.at(-1)?.content).toBe("One more thing, said in full.");
  expect(view.errors).toEqual([]);
  view.unmount();
});

test("Stop Answering withdraws the question, and what had come is kept as cut short", async () => {
  model.calls.length = 0;
  const conversation = begin("What is long?");
  const view = await show(createElement(Chat, { conversation }));
  model.calls[0].onText("Everything that");
  await settle(200);
  expect(view.of("Action").map((a) => a.props.title)).toContain("Stop Answering");
  dispatch(view.of("Action").find((a) => a.props.title === "Stop Answering")!.id, "onAction", []);
  await settle(100);
  expect(model.calls[0].signal.aborted).toBe(true);
  const detail = view.of("List.Item.Detail")[0].props.markdown;
  expect(detail).toBe("> What is long?\n\nEverything that\n\n*The answer was cut short.*");
  // Stopped by the user: not a failure to report.
  expect(detail).not.toContain("Could not get an answer");
  expect(chats().store.get(conversation.id)?.messages.at(-1)).toMatchObject({ role: "assistant", content: "Everything that", partial: true });
  expect(view.errors).toEqual([]);
  view.unmount();
});

test("an answer goes on being written, and is kept, with the view gone", async () => {
  model.calls.length = 0;
  const conversation = begin("What is kept?");
  const view = await show(createElement(Chat, { conversation }));
  model.calls[0].onText("Every");
  await settle(200);
  expect(view.of("List.Item.Detail")[0].props.markdown).toBe("> What is kept?\n\nEvery");
  // The palette closes.
  view.unmount();
  await settle();
  model.calls[0].finish("Everything.");
  await settle();
  expect(chats().store.get(conversation.id)?.messages.map((m) => m.content)).toEqual(["What is kept?", "Everything."]);
});

test("the command lists what is kept, the latest first, and finds by what was said", async () => {
  chats().store.removeAll();
  const older = answered(asked(newConversation(1000), "Sourdough schedule", 1000), "Feed the starter.", 1001);
  const newer = answered(asked(newConversation(2000), "Borrow checker", 2000), "References borrow.", 2001);
  chats().store.save(older);
  chats().store.save(newer);

  const view = await show(createElement(AiChat, {}));
  expect(view.errors).toEqual([]);
  expect(view.of("List.Item").map((i) => i.props.title)).toEqual(["Borrow checker", "Sourdough schedule"]);
  for (const title of ["Open Conversation", "New Chat", "Rename Conversation", "Copy Last Answer", "Copy Conversation as Markdown", "Delete Conversation", "Delete All Conversations"]) {
    expect(view.of("Action").map((a) => a.props.title)).toContain(title);
  }

  dispatch(view.of("List")[0].id, "onSearchTextChange", ["starter"]);
  await settle();
  expect(view.of("List.Item").map((i) => i.props.title)).toEqual(["Sourdough schedule"]);

  // Opened, it is the conversation, to continue.
  const open = view.of("Action").find((a) => a.props.title === "Open Conversation")!;
  dispatch(open.id, "onAction", []);
  await settle();
  expect(view.of("List")[0].props.navigationTitle).toBe("Sourdough schedule");
  expect(view.of("List.Item.Detail")[0].props.markdown).toBe("> Sourdough schedule\n\nFeed the starter.");
  view.unmount();
});
