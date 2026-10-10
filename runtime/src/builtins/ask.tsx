//! Ask AI: a question from the root search (Tab), answered in place. The
//! question starts a conversation that is kept, and can be continued in AI
//! Chat (see chat.tsx).

import { useState } from "react";

import { launchCommand } from "../api";
import { Action, ActionPanel, Detail } from "../api/components";
import { Icon } from "../api/icon";
import { useNavigation } from "../api/navigation";
import { Clipboard, pasteable, showHUD } from "../api/system";
import { begin, Question, useChat } from "./chat";
import { exchangeMarkdown, exchanges, type Conversation } from "./conversations";

function Answer(props: { conversation: Conversation }) {
  const chat = useChat(props.conversation);
  const exchange = exchanges(chat.conversation.messages).at(-1);
  const answer = exchange?.answer?.content;
  const busy = chat.writing !== null;

  return (
    <Detail
      navigationTitle="Ask AI"
      isLoading={busy}
      markdown={exchange ? exchangeMarkdown(exchange, chat.writing, chat.error) : ""}
      // Model output: no raw HTML, no images fetched from wherever it points.
      {...{ untrusted: true }}
      actions={
        <ActionPanel>
          {!busy && (
            <Action
              title="Continue in AI Chat"
              icon={Icon.SpeechBubble}
              // AI Chat opens on this conversation, with a bar to reply in.
              onAction={() => launchCommand({ name: "ai-chat", context: { conversation: chat.conversation.id } })}
            />
          )}
          {answer && (
            <Action
              title="Copy Answer"
              icon={Icon.CopyClipboard}
              shortcut={{ modifiers: ["cmd", "shift"], key: "c" }}
              onAction={async () => {
                await Clipboard.copy(answer);
                await showHUD("Copied to Clipboard");
              }}
            />
          )}
          {answer && <Action title="Paste Answer" icon={Icon.Clipboard} shortcut={{ modifiers: ["cmd", "shift"], key: "v" }} onAction={() => Clipboard.paste(pasteable(answer))} />}
          {busy ? (
            <Action title="Stop Answering" icon={Icon.Stop} shortcut={{ modifiers: ["cmd", "shift"], key: "s" }} onAction={chat.stop} />
          ) : (
            <Action title="Try Again" icon={Icon.ArrowClockwise} shortcut={{ modifiers: ["cmd"], key: "r" }} onAction={chat.again} />
          )}
        </ActionPanel>
      }
    />
  );
}

export function AskAi(props: { fallbackText?: string }) {
  const { push } = useNavigation();
  // Asked once, when the command starts: not again each time this renders.
  const [asked] = useState(() => {
    const question = props.fallbackText?.trim();
    return question ? begin(question) : undefined;
  });
  if (asked) return <Answer conversation={asked} />;
  return <Question title="Ask AI" onSubmit={(question) => push(<Answer conversation={begin(question)} />)} />;
}
