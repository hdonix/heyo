//! AI Chat: the conversations with the AI model, kept on this computer, and
//! the view in which one is continued.
//!
//! A conversation is shown as a list of its questions, the latest first,
//! with the answer to the selected one beside it, and the search bar is where the next message is
//! typed. Each answer is a node of its own, so while the model writes, the
//! one answer being written is all that is sent to the window and drawn
//! again, however long the conversation.

import path from "node:path";
import { memo, useEffect, useMemo, useRef, useState } from "react";

import { describeClaudeError, pasteable, streamClaude } from "../ai";
import { Action, ActionPanel, Form, List } from "../api/components";
import { Icon } from "../api/icon";
import { useNavigation } from "../api/navigation";
import { Alert, clearSearchBar, Clipboard, confirmAlert, showHUD, showToast, Toast } from "../api/system";
import { supportPath } from "../context";
import { answersFor, type Answers } from "./answering";
import {
  asked,
  asMarkdown,
  conversationsIn,
  exchangeMarkdown,
  exchanges,
  found,
  lastAnswer,
  newConversation,
  renamed,
  unanswered,
  type Conversation,
  type Conversations,
  type Exchange,
} from "./conversations";

const SYSTEM = `You are the quick-answer assistant inside a desktop launcher. Answer the question directly and briefly: lead with the answer, then only the explanation that is needed. Use Markdown, with code blocks for code and commands. Answer in the language of the question. If you are not sure, say so rather than guess.`;

let own: { store: Conversations; answers: Answers } | undefined;

/// The conversations Heyo keeps, and the answers being written for them.
export function chats() {
  if (!own) {
    const store = conversationsIn(path.join(supportPath("@heyo"), "conversations"));
    own = { store, answers: answersFor(store, (options) => streamClaude({ system: SYSTEM, ...options }), describeClaudeError) };
  }
  return own;
}

/// Starts a conversation with a question, which the model sets about answering.
export function begin(question: string): Conversation {
  const { store, answers } = chats();
  const now = Date.now();
  const conversation = asked(newConversation(now), question, now);
  store.save(conversation);
  answers.start(conversation);
  return conversation;
}

/// A conversation on screen: as it is kept, and the answer being written.
export function useChat(initial: Conversation) {
  const { store, answers } = chats();
  const [conversation, setConversation] = useState(initial);
  const [answer, setAnswer] = useState(() => answers.of(initial.id));
  const [written, setWritten] = useState(answer?.text ?? "");
  const [error, setError] = useState<string>();
  const latest = useRef(conversation);
  latest.current = conversation;

  useEffect(() => {
    if (!answer) return;
    let watching = true;
    // Streamed text arrives faster than it is worth re-rendering.
    const timer = setInterval(() => setWritten(answer.text), 80);
    answer.settled.then((outcome) => {
      if (!watching) return;
      clearInterval(timer);
      if (outcome.conversation) setConversation(outcome.conversation);
      setError(outcome.error);
      setAnswer(undefined);
      setWritten("");
    });
    return () => {
      watching = false;
      clearInterval(timer);
    };
  }, [answer]);

  const start = (next: Conversation) => {
    store.save(next);
    setConversation(next);
    setError(undefined);
    setWritten("");
    setAnswer(answers.start(next));
  };

  return {
    conversation,
    /// The answer so far while the model is writing; null when it is not.
    writing: answer ? written : null,
    error,
    ask: (question: string) => start(asked(latest.current, question, Date.now())),
    /// Has the last question answered again (or at last).
    again: () => {
      const before = unanswered(latest.current);
      if (!answers.of(before.id) && before.messages.at(-1)?.role === "user") start(before);
    },
    stop: () => answers.stop(latest.current.id),
    rename: (title: string) => {
      const next = renamed(latest.current, title);
      // One not spoken in yet is not kept yet either.
      if (next.messages.length) store.save(next);
      setConversation(next);
    },
  };
}

type Commands = Record<"send" | "compose" | "stop" | "again" | "copyAll" | "rename" | "startNew" | "remove", () => void>;

/// The same functions for as long as the view lives, doing what the latest
/// render says: rows given these need not render again when it changes.
function useStable(commands: Commands): Commands {
  const latest = useRef(commands);
  latest.current = commands;
  return useMemo(() => Object.fromEntries(Object.keys(commands).map((name) => [name, () => latest.current[name as keyof Commands]()])) as Commands, []);
}

async function copy(text: string) {
  await Clipboard.copy(text);
  await showHUD("Copied to Clipboard");
}

function ChatActions(props: { exchange?: Exchange; last: boolean; busy: boolean; commands: Commands }) {
  const { exchange, last, busy, commands } = props;
  const answer = exchange?.answer?.content;
  return (
    <ActionPanel>
      <ActionPanel.Section>
        <Action title="Send Message" icon={Icon.SpeechBubble} onAction={commands.send} />
        <Action title="Write a Longer Message" icon={Icon.Pencil} shortcut={{ modifiers: ["cmd", "shift"], key: "e" }} onAction={commands.compose} />
        {busy && <Action title="Stop Answering" icon={Icon.Stop} shortcut={{ modifiers: ["cmd", "shift"], key: "s" }} onAction={commands.stop} />}
      </ActionPanel.Section>
      {exchange && (
        <ActionPanel.Section>
          {answer && <Action title="Copy Answer" icon={Icon.CopyClipboard} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} onAction={() => copy(answer)} />}
          {answer && (
            <Action title="Paste Answer" icon={Icon.Clipboard} shortcut={{ modifiers: ["cmd", "shift"], key: "v" }} onAction={() => Clipboard.paste(pasteable(answer))} />
          )}
          {last && !busy && (
            <Action title={answer ? "Answer Again" : "Try Again"} icon={Icon.ArrowClockwise} shortcut={{ modifiers: ["cmd"], key: "r" }} onAction={commands.again} />
          )}
        </ActionPanel.Section>
      )}
      <ActionPanel.Section>
        {exchange && (
          <Action title="Copy Conversation as Markdown" icon={Icon.CopyClipboard} shortcut={{ modifiers: ["cmd", "shift"], key: "m" }} onAction={commands.copyAll} />
        )}
        <Action title="Rename Conversation" icon={Icon.Pencil} shortcut={{ modifiers: ["cmd", "shift"], key: "r" }} onAction={commands.rename} />
        <Action title="New Chat" icon={Icon.Plus} shortcut={{ modifiers: ["cmd", "shift"], key: "n" }} onAction={commands.startNew} />
        {exchange && (
          <Action title="Delete Conversation" icon={Icon.Trash} style="destructive" shortcut={{ modifiers: ["ctrl"], key: "x" }} onAction={commands.remove} />
        )}
      </ActionPanel.Section>
    </ActionPanel>
  );
}

const rowId = (index: number) => `exchange-${index}`;

/// The start of a text, on one line.
const oneLine = (text: string, length: number) => Array.from(text.replace(/\s+/g, " ").trim()).slice(0, length).join("");

/// One question in the list, with its answer as the detail beside it. Only
/// the row whose answer is being written has anything changing.
const Row = memo(function Row(props: { index: number; exchange: Exchange; last: boolean; writing: string | null; error?: string; commands: Commands }) {
  const { exchange, writing } = props;
  const icon = exchange.answer ? Icon.SpeechBubble : writing !== null ? Icon.SpeechBubbleActive : Icon.SpeechBubbleImportant;
  return (
    <List.Item
      id={rowId(props.index)}
      title={oneLine(exchange.question.content, 200)}
      icon={icon}
      // Model output: no raw HTML, no images fetched from wherever it points.
      detail={<List.Item.Detail markdown={exchangeMarkdown(exchange, writing, props.error)} {...{ untrusted: true }} />}
      actions={<ChatActions exchange={exchange} last={props.last} busy={writing !== null} commands={props.commands} />}
    />
  );
});

/// A form for one long text: a message, or (on one line) a title.
function TextForm(props: { navigationTitle: string; title: string; submit: string; placeholder?: string; initial?: string; multiline?: boolean; onSubmit(text: string): void }) {
  const [error, setError] = useState<string>();
  const Field = props.multiline ? Form.TextArea : Form.TextField;
  return (
    <Form
      navigationTitle={props.navigationTitle}
      actions={
        <ActionPanel>
          <Action.SubmitForm
            title={props.submit}
            onSubmit={(values: { text: string }) => {
              const text = values.text.trim();
              if (!text) return setError("Required");
              props.onSubmit(text);
            }}
          />
        </ActionPanel>
      }
    >
      <Field id="text" title={props.title} placeholder={props.placeholder} defaultValue={props.initial} error={error} autoFocus />
    </Form>
  );
}

/// The form a question is written in when there is no search bar for it.
export function Question(props: { title: string; onSubmit(question: string): void }) {
  return <TextForm navigationTitle={props.title} title="Question" submit="Ask" placeholder="Ask anything…" multiline onSubmit={props.onSubmit} />;
}

/// One conversation, to read and to continue.
export function Chat(props: { conversation: Conversation }) {
  const { store, answers } = chats();
  const chat = useChat(props.conversation);
  const { conversation, writing } = chat;
  const { push, pop } = useNavigation();
  // What is typed in the search bar: the next message. Kept out of the
  // state, so that typing renders nothing here.
  const draft = useRef("");
  const all = useMemo(() => exchanges(conversation.messages), [conversation.messages]);
  // The row to select: the latest, and each new question as it is asked.
  const [wanted, setWanted] = useState(() => rowId(all.length - 1));

  const send = (text: string) => {
    const question = text.trim();
    if (!question) {
      return void showToast({ style: Toast.Style.Failure, title: "Nothing to send", message: "Type a message in the search bar, then press Enter." });
    }
    if (writing !== null) {
      return void showToast({ style: Toast.Style.Failure, title: "Still answering", message: "Wait for the answer, or stop it." });
    }
    draft.current = "";
    clearSearchBar();
    setWanted(rowId(all.length));
    chat.ask(question);
  };
  const sendLatest = useRef(send);
  sendLatest.current = send;

  const commands = useStable({
    send: () => send(draft.current),
    compose: () =>
      push(
        <Question
          title="Message"
          onSubmit={(question) => {
            pop();
            // The answer awaited when the form opened may have come since.
            sendLatest.current(question);
          }}
        />,
      ),
    stop: chat.stop,
    again: chat.again,
    copyAll: () => void copy(asMarkdown(conversation)),
    rename: () =>
      push(
        <TextForm
          navigationTitle="Rename Conversation"
          title="Title"
          submit="Rename"
          initial={conversation.title}
          onSubmit={(title) => {
            chat.rename(title);
            pop();
          }}
        />,
      ),
    startNew: () => push(<Chat conversation={newConversation(Date.now())} />),
    remove: async () => {
      if (!(await confirmDelete(`Delete "${conversation.title}"?`, "Delete"))) return;
      answers.drop(conversation.id);
      store.remove(conversation.id);
      pop();
    },
  });

  return (
    <List
      navigationTitle={conversation.title}
      searchBarPlaceholder={all.length ? "Reply…" : "Ask anything…"}
      filtering={false}
      onSearchTextChange={(text: string) => (draft.current = text)}
      isLoading={writing !== null}
      isShowingDetail={all.length > 0}
      selectedItemId={wanted}
      actions={<ChatActions last={false} busy={false} commands={commands} />}
    >
      <List.EmptyView icon={Icon.SpeechBubble} title="Ask anything" description="Type a message and press Enter." />
      {/* The latest first: it is the one being read and replied to, and
          the first row is where the selection is whenever something is typed. */}
      {all
        .map((exchange, i) => {
          const last = i === all.length - 1;
          return <Row key={i} index={i} exchange={exchange} last={last} writing={last ? writing : null} error={last ? chat.error : undefined} commands={commands} />;
        })
        .reverse()}
    </List>
  );
}

const confirmDelete = (title: string, action: string) =>
  confirmAlert({ title, message: "This cannot be undone.", primaryAction: { title: action, style: Alert.ActionStyle.Destructive } });

/// The first words of the last thing said, on one line.
function preview(conversation: Conversation) {
  return oneLine((lastAnswer(conversation) ?? "").replace(/[#*`>_]/g, ""), 80);
}

/// The command: every conversation kept, the latest first.
export function AiChat(props: { launchContext?: { conversation?: string } }) {
  const { store, answers } = chats();
  const [all, setAll] = useState(() => store.list());
  const [query, setQuery] = useState("");
  const { push, pop } = useNavigation();
  const reload = () => setAll(store.list());
  const shown = useMemo(() => found(all, query), [all, query]);

  const open = (conversation: Conversation) => push(<Chat conversation={store.get(conversation.id) ?? conversation} />, reload);
  const startNew = () => push(<Chat conversation={newConversation(Date.now())} />, reload);

  // "Continue in AI Chat" from an answer of Ask AI: straight into it.
  useEffect(() => {
    const id = props.launchContext?.conversation;
    const conversation = id ? all.find((c) => c.id === id) : undefined;
    if (conversation) open(conversation);
  }, []);

  const rename = (conversation: Conversation) =>
    push(
      <TextForm
        navigationTitle="Rename Conversation"
        title="Title"
        submit="Rename"
        initial={conversation.title}
        onSubmit={(title) => {
          // As it is kept now: an answer may have arrived meanwhile.
          store.save(renamed(store.get(conversation.id) ?? conversation, title));
          reload();
          pop();
        }}
      />,
    );

  const remove = async (conversation: Conversation) => {
    if (!(await confirmDelete(`Delete "${conversation.title}"?`, "Delete"))) return;
    answers.drop(conversation.id);
    store.remove(conversation.id);
    reload();
  };

  const removeAll = async () => {
    const count = all.length;
    if (!(await confirmDelete(count === 1 ? "Delete the conversation?" : `Delete all ${count} conversations?`, "Delete All"))) return;
    answers.drop();
    store.removeAll();
    reload();
    await showToast({ title: "Conversations deleted" });
  };

  const newChat = <Action title="New Chat" icon={Icon.Plus} shortcut={{ modifiers: ["cmd", "shift"], key: "n" }} onAction={startNew} />;

  return (
    <List
      searchBarPlaceholder="Search conversations…"
      filtering={false}
      onSearchTextChange={setQuery}
      actions={<ActionPanel>{newChat}</ActionPanel>}
    >
      <List.EmptyView
        icon={Icon.SpeechBubble}
        title={all.length ? "No conversation found" : "No conversations yet"}
        description={all.length ? "Nothing said matches these words." : "Press Enter to start one, or type a question in the root search and press Tab."}
      />
      {shown.map((conversation) => {
        const answer = lastAnswer(conversation);
        return (
          <List.Item
            key={conversation.id}
            id={conversation.id}
            title={conversation.title}
            subtitle={preview(conversation)}
            icon={answers.of(conversation.id) ? Icon.SpeechBubbleActive : Icon.SpeechBubble}
            accessories={[{ date: new Date(conversation.updatedAt) }]}
            actions={
              <ActionPanel>
                <ActionPanel.Section>
                  <Action title="Open Conversation" icon={Icon.SpeechBubble} onAction={() => open(conversation)} />
                  {newChat}
                </ActionPanel.Section>
                <ActionPanel.Section>
                  {answer && <Action title="Copy Last Answer" icon={Icon.CopyClipboard} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} onAction={() => copy(answer)} />}
                  <Action
                    title="Copy Conversation as Markdown"
                    icon={Icon.CopyClipboard}
                    shortcut={{ modifiers: ["cmd", "shift"], key: "m" }}
                    onAction={() => copy(asMarkdown(conversation))}
                  />
                  <Action title="Rename Conversation" icon={Icon.Pencil} shortcut={{ modifiers: ["cmd", "shift"], key: "r" }} onAction={() => rename(conversation)} />
                </ActionPanel.Section>
                <ActionPanel.Section>
                  <Action title="Delete Conversation" icon={Icon.Trash} style="destructive" shortcut={{ modifiers: ["ctrl"], key: "x" }} onAction={() => remove(conversation)} />
                  <Action
                    title="Delete All Conversations"
                    icon={Icon.Trash}
                    style="destructive"
                    shortcut={{ modifiers: ["ctrl", "shift"], key: "x" }}
                    onAction={removeAll}
                  />
                </ActionPanel.Section>
              </ActionPanel>
            }
          />
        );
      })}
    </List>
  );
}
