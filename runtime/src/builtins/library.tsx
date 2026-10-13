//! Quicklinks and snippets: the "Create …" forms and "Search …" lists. The
//! root search reads the same JSON files (see src-tauri/src/library.rs).

import { randomUUID } from "node:crypto";
import path from "node:path";
import { useState } from "react";

import { expand, needsArgument } from "../../../shared/placeholders";
import { Action, ActionPanel, Form, List } from "../api/components";
import { Icon } from "../api/icon";
import { useNavigation } from "../api/navigation";
import {
  Clipboard,
  closeMainWindow,
  confirmAlert,
  open,
  pasteKeystroke,
  popToRoot,
  showHUD,
  showToast,
} from "../api/system";
import { extensionsDir, readJson, writeJson } from "../context";
import { request } from "../protocol";
import { t } from "../../../shared/i18n";

interface Quicklink {
  id: string;
  name: string;
  link: string;
  icon?: string;
}

interface Snippet {
  id: string;
  name: string;
  text: string;
  keyword?: string;
}

const file = (name: string) => path.join(path.dirname(extensionsDir), name);
const QUICKLINKS = file("quicklinks.json");
const SNIPPETS = file("snippets.json");

function useStore<T extends { id: string }>(file: string) {
  const [items, setItems] = useState<T[]>(() => readJson<T[]>(file, []));
  const save = async (next: T[]) => {
    writeJson(file, next);
    setItems(next);
    await request("host.reload");
  };
  return {
    items,
    upsert: (item: T) => {
      const exists = items.some((i) => i.id === item.id);
      return save(exists ? items.map((i) => (i.id === item.id ? item : i)) : [...items, item]);
    },
    remove: (id: string) => save(items.filter((i) => i.id !== id)),
  };
}

const readClipboard = () => Clipboard.readText().then((text) => text ?? "");

/// What a link or a snippet can have filled in. The placeholders are
/// written as they are typed, in every language.
const placeholderHelp = () => t("Placeholders: {0} (text you type), {1}.", "{query}", '{clipboard}, {date}, {time}, {datetime}, {day}, {uuid}, {date format="yyyy-MM-dd"}');

// --- Quicklinks --------------------------------------------------------------

/// `quicklink` is one being edited; `draft` fills in a new one (an extension's
/// `Action.CreateQuicklink`).
export function QuicklinkForm(props: { quicklink?: Quicklink; draft?: { name?: string; link?: string }; onSaved?(): void }) {
  const store = useStore<Quicklink>(QUICKLINKS);
  const [nameError, setNameError] = useState<string>();
  const [linkError, setLinkError] = useState<string>();

  const submit = async (values: { name: string; link: string }) => {
    const name = values.name.trim();
    const link = values.link.trim();
    setNameError(name ? undefined : t("Required"));
    setLinkError(link ? undefined : t("Required"));
    if (!name || !link) return;
    await store.upsert({ ...props.quicklink, id: props.quicklink?.id ?? randomUUID(), name, link });
    await showToast({ title: props.quicklink ? t("Quicklink saved") : t("Quicklink created"), message: name });
    if (props.onSaved) props.onSaved();
    else await popToRoot();
  };

  return (
    <Form
      navigationTitle={props.quicklink ? t("Edit Quicklink") : t("Create Quicklink")}
      actions={
        <ActionPanel>
          <Action.SubmitForm title={props.quicklink ? t("Save Quicklink") : t("Create Quicklink")} onSubmit={submit} />
        </ActionPanel>
      }
    >
      <Form.TextField id="name" title={t("Name")} placeholder={t("Search GitHub")} defaultValue={props.quicklink?.name ?? props.draft?.name} error={nameError} autoFocus />
      <Form.TextField
        id="link"
        title={t("Link")}
        placeholder="https://github.com/search?q={query}"
        defaultValue={props.quicklink?.link ?? props.draft?.link}
        error={linkError}
        info={placeholderHelp()}
      />
      <Form.Description text={t("Tip: give it an alias in the root search (Ctrl+K → Set Alias), then type “<alias> <text>” to open it directly. {0}", placeholderHelp())} />
    </Form>
  );
}

export function CreateQuicklink() {
  return <QuicklinkForm />;
}

async function openQuicklink(quicklink: Quicklink, query?: string) {
  const isUrl = /^[a-z][\w+.-]*:/i.test(quicklink.link);
  await open(await expand(quicklink.link, { query, encode: isUrl, readClipboard }));
  await closeMainWindow();
}

function QuicklinkQuery({ quicklink }: { quicklink: Quicklink }) {
  return (
    <Form
      navigationTitle={quicklink.name}
      actions={
        <ActionPanel>
          <Action.SubmitForm title={t("Open Quicklink")} onSubmit={(v: { query: string }) => openQuicklink(quicklink, v.query)} />
        </ActionPanel>
      }
    >
      <Form.TextField id="query" title={t("Query")} placeholder={quicklink.link} autoFocus />
    </Form>
  );
}

export function SearchQuicklinks() {
  const store = useStore<Quicklink>(QUICKLINKS);
  const { pop } = useNavigation();

  return (
    <List searchBarPlaceholder={t("Search quicklinks…")}>
      <List.EmptyView
        icon={Icon.Link}
        title={t("No quicklinks yet")}
        description={t("Press Enter to create one.")}
        actions={
          <ActionPanel>
            <Action.Push title={t("Create Quicklink")} icon={Icon.Plus} target={<QuicklinkForm onSaved={pop} />} />
          </ActionPanel>
        }
      />
      {store.items.map((q) => (
        <List.Item
          key={q.id}
          title={q.name}
          subtitle={q.link}
          icon={Icon.Link}
          keywords={[q.link]}
          actions={
            <ActionPanel>
              <ActionPanel.Section>
                {needsArgument(q.link) ? (
                  <Action.Push title={t("Open Quicklink")} icon={Icon.Globe} target={<QuicklinkQuery quicklink={q} />} />
                ) : (
                  <Action title={t("Open Quicklink")} icon={Icon.Globe} onAction={() => openQuicklink(q)} />
                )}
                <Action.CopyToClipboard title={t("Copy Link")} content={q.link} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} />
              </ActionPanel.Section>
              <ActionPanel.Section>
                <Action.Push
                  title={t("Edit Quicklink")}
                  icon={Icon.Pencil}
                  shortcut={{ modifiers: ["cmd"], key: "e" }}
                  target={<QuicklinkForm quicklink={q} onSaved={pop} />}
                />
                <Action.Push
                  title={t("Create Quicklink")}
                  icon={Icon.Plus}
                  shortcut={{ modifiers: ["cmd"], key: "n" }}
                  target={<QuicklinkForm onSaved={pop} />}
                />
                <Action
                  title={t("Delete Quicklink")}
                  icon={Icon.Trash}
                  style="destructive"
                  shortcut={{ modifiers: ["ctrl"], key: "x" }}
                  onAction={async () => {
                    if (await confirmAlert({ title: t("Delete “{0}”?", q.name), primaryAction: { title: t("Delete"), style: "DESTRUCTIVE" } })) {
                      await store.remove(q.id);
                    }
                  }}
                />
              </ActionPanel.Section>
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}

// --- Snippets ----------------------------------------------------------------

const keywordHelp = () =>
  t(
    "Typing the keyword in the root search selects this snippet. With “Expand Snippets as You Type” on (Heyo Settings), typing it in any application replaces it with the snippet: at once if it begins or ends with a symbol (;sig), after the next space or punctuation mark if it is a plain word (sig). 2 to 32 characters, capitals matter.",
  );

/// `snippet` is one being edited; `draft` fills in a new one (an extension's
/// `Action.CreateSnippet`).
export function SnippetForm(props: { snippet?: Snippet; draft?: { name?: string; text?: string; keyword?: string }; onSaved?(): void }) {
  const store = useStore<Snippet>(SNIPPETS);
  const [nameError, setNameError] = useState<string>();
  const [textError, setTextError] = useState<string>();

  const submit = async (values: { name: string; text: string; keyword: string }) => {
    const name = values.name.trim();
    setNameError(name ? undefined : t("Required"));
    setTextError(values.text ? undefined : t("Required"));
    if (!name || !values.text) return;
    const keyword = values.keyword.trim() || undefined;
    await store.upsert({ id: props.snippet?.id ?? randomUUID(), name, text: values.text, keyword });
    await showToast({ title: props.snippet ? t("Snippet saved") : t("Snippet created"), message: name });
    if (props.onSaved) props.onSaved();
    else await popToRoot();
  };

  return (
    <Form
      navigationTitle={props.snippet ? t("Edit Snippet") : t("Create Snippet")}
      actions={
        <ActionPanel>
          <Action.SubmitForm title={props.snippet ? t("Save Snippet") : t("Create Snippet")} onSubmit={submit} />
        </ActionPanel>
      }
    >
      <Form.TextField id="name" title={t("Name")} placeholder={t("Email signature")} defaultValue={props.snippet?.name ?? props.draft?.name} error={nameError} autoFocus />
      <Form.TextArea id="text" title={t("Snippet")} defaultValue={props.snippet?.text ?? props.draft?.text} error={textError} info={placeholderHelp()} />
      <Form.TextField
        id="keyword"
        title={t("Keyword")}
        placeholder="sig"
        defaultValue={props.snippet?.keyword ?? props.draft?.keyword}
        info={keywordHelp()}
      />
    </Form>
  );
}

export function CreateSnippet() {
  return <SnippetForm />;
}

async function deliverSnippet(snippet: Snippet, paste: boolean) {
  const text = await expand(snippet.text, { readClipboard });
  if (paste) {
    await Clipboard.copy(text);
    await closeMainWindow();
    await pasteKeystroke();
  } else {
    await Clipboard.copy(text);
    await showHUD(t("Copied to Clipboard"));
  }
}

export function SearchSnippets() {
  const store = useStore<Snippet>(SNIPPETS);
  const { pop } = useNavigation();

  return (
    <List searchBarPlaceholder={t("Search snippets…")} isShowingDetail={store.items.length > 0}>
      <List.EmptyView
        icon={Icon.Snippets}
        title={t("No snippets yet")}
        description={t("Press Enter to create one.")}
        actions={
          <ActionPanel>
            <Action.Push title={t("Create Snippet")} icon={Icon.Plus} target={<SnippetForm onSaved={pop} />} />
          </ActionPanel>
        }
      />
      {store.items.map((s) => (
        <List.Item
          key={s.id}
          title={s.name}
          icon={Icon.Snippets}
          keywords={[s.text, ...(s.keyword ? [s.keyword] : [])]}
          accessories={s.keyword ? [{ tag: s.keyword }] : undefined}
          detail={
            <List.Item.Detail
              markdown={"```\n" + s.text.replace(/```/g, "ˋˋˋ") + "\n```"}
              metadata={
                <List.Item.Detail.Metadata>
                  <List.Item.Detail.Metadata.Label title={t("Name")} text={s.name} />
                  {s.keyword && <List.Item.Detail.Metadata.Label title={t("Keyword")} text={s.keyword} />}
                  <List.Item.Detail.Metadata.Label title={t("Characters")} text={String([...s.text].length)} />
                </List.Item.Detail.Metadata>
              }
            />
          }
          actions={
            <ActionPanel>
              <ActionPanel.Section>
                <Action title={t("Paste Snippet")} icon={Icon.Clipboard} onAction={() => deliverSnippet(s, true)} />
                <Action title={t("Copy Snippet")} icon={Icon.CopyClipboard} onAction={() => deliverSnippet(s, false)} />
              </ActionPanel.Section>
              <ActionPanel.Section>
                <Action.Push
                  title={t("Edit Snippet")}
                  icon={Icon.Pencil}
                  shortcut={{ modifiers: ["cmd"], key: "e" }}
                  target={<SnippetForm snippet={s} onSaved={pop} />}
                />
                <Action.Push
                  title={t("Create Snippet")}
                  icon={Icon.Plus}
                  shortcut={{ modifiers: ["cmd"], key: "n" }}
                  target={<SnippetForm onSaved={pop} />}
                />
                <Action
                  title={t("Delete Snippet")}
                  icon={Icon.Trash}
                  style="destructive"
                  shortcut={{ modifiers: ["ctrl"], key: "x" }}
                  onAction={async () => {
                    if (await confirmAlert({ title: t("Delete “{0}”?", s.name), primaryAction: { title: t("Delete"), style: "DESTRUCTIVE" } })) {
                      await store.remove(s.id);
                    }
                  }}
                />
              </ActionPanel.Section>
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}
