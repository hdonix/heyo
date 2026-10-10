//! Clipboard History, modeled on Raycast's: entries on the left, a preview
//! with metadata on the right. The history itself is recorded by the host.
//!
//! What it shows it reads as an extension would, through `@heyo/api`
//! (`Memory.search`, `ClipboardHistory.copy`). The keeping of the history is
//! not an extension's to do, and is asked of the host directly: pinning and
//! deleting entries, pausing the recording, whether it is recording at all,
//! and telling the host which result was used (`host.rs`).

import { useEffect, useMemo, useRef, useState } from "react";

import { Color } from "../api";
import { Action, ActionPanel, List } from "../api/components";
import { Icon } from "../api/icon";
import { closeMainWindow, confirmAlert, pasteKeystroke, showHUD, showToast, Toast } from "../api/system";
import type { Kind } from "../../../shared/bindings/Kind";
import { ClipboardHistory as History, Memory, type MemoryItem, type MemoryResult } from "../heyo/memory";
import { request } from "../protocol";

/// An entry of the history, as any extension is told of one.
type Entry = MemoryItem;

/// How the keeping of the history stands (`host.clipboardHistory.status`).
interface Status {
  /// Whether copies are recorded at all, and whether that is paused for now.
  recording?: boolean;
  paused?: boolean;
  /// While finding by meaning is on: how many entries can be found that way,
  /// and how many are yet to be read.
  meaning?: { indexed: number; waiting: number };
}

/// What a search found, how the host read the query (a time ("昨天下午"), an
/// application ("from firefox") or a kind ("链接") taken out of it), and
/// how the keeping stood when it was asked.
type Found = MemoryResult & Status;

/// How many entries the view shows: as many as a search gives at most.
const SHOWN = 300;

/// How long typing must pause before the meaning of a query is asked for
/// (the answer then waits for the model, which takes a second or two to
/// start and a few milliseconds after that).
const MEANING_PAUSE = 150;

/// The entry's number in the history, which is what its keeping goes by.
const numberOf = (entry: Entry) => Number(entry.id.slice(entry.id.indexOf(":") + 1));

const KIND_LABELS: Record<Entry["kind"], string> = {
  text: "Text",
  link: "Link",
  color: "Color",
  file: "File",
  image: "Image",
};

const FILTERS = [
  { value: "all", title: "All Types" },
  { value: "text", title: "Text" },
  { value: "image", title: "Images" },
  { value: "link", title: "Links" },
  { value: "color", title: "Colors" },
  { value: "file", title: "Files" },
];

function title(entry: Entry) {
  if (entry.kind === "image") return `Image (${entry.width}×${entry.height})`;
  const line = (entry.text ?? "").trim().split("\n")[0];
  return line.length > 80 ? `${line.slice(0, 80)}…` : line;
}

/// What stands for the kind of an entry.
function kindIcon(entry: Entry) {
  switch (entry.kind) {
    case "link":
      return Icon.Link;
    case "file":
      return Icon.Document;
    default:
      return Icon.Text;
  }
}

/// The icon of a row. An image and a color show themselves; anything else
/// shows where it came from, which says more than that it is text. (A row
/// beside the detail pane has room for an icon and a title only.)
function icon(entry: Entry) {
  if (entry.kind === "image") return entry.image!;
  if (entry.kind === "color") return { source: Icon.CircleFilled, tintColor: entry.text!.trim() };
  return entry.appIcon ? { source: entry.appIcon, fallback: kindIcon(entry) } : kindIcon(entry);
}

/// The application an entry came from, at the end of its row.
function accessories(entry: Entry) {
  // Said, since the row itself does not show why it is there.
  const meant = entry.byMeaning ? [{ icon: Icon.Stars, tooltip: "Found by its meaning: the words you typed are not in it" }] : [];
  const source = entry.appName ? [{ icon: entry.appIcon, text: entry.appIcon ? undefined : entry.appName, tooltip: `Copied from ${entry.appName}` }] : [];
  const pin = entry.pinned ? [{ icon: { source: Icon.Pin, tintColor: Color.Orange } }] : [];
  return meant.length || source.length || pin.length ? [...meant, ...source, ...pin] : undefined;
}

/// A Markdown code fence longer than any backtick run in the text.
function fenced(text: string) {
  const longest = Math.max(2, ...(text.match(/`+/g) ?? []).map((m) => m.length));
  const fence = "`".repeat(longest + 1);
  return `${fence}\n${text}\n${fence}`;
}

function preview(entry: Entry) {
  switch (entry.kind) {
    case "image":
      return `![](${encodeURI(entry.image!)})`;
    case "color":
      return `<div style="height:140px;border-radius:10px;background:${entry.text!.trim().replace(/[";<>]/g, "")}"></div>\n\n${fenced(entry.text!)}`;
    default:
      return fenced(entry.text ?? "");
  }
}

function section(entry: Entry, now: Date) {
  if (entry.pinned) return "Pinned";
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (entry.at >= startOfToday) return "Today";
  if (entry.at >= startOfToday - 86_400_000) return "Yesterday";
  if (entry.at >= startOfToday - 6 * 86_400_000) return "This Week";
  return "Older";
}

const SECTIONS = ["Pinned", "Today", "Yesterday", "This Week", "Older"];

function formatTime(ms: number) {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/// How the query was read, for the heading of the results.
function reading(query: Found["understood"]) {
  const parts: string[] = [];
  if (query.from !== undefined && query.to !== undefined) {
    const [from, to] = [new Date(query.from), new Date(query.to - 1)];
    const day: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" };
    const whole = from.getHours() === 0 && from.getMinutes() === 0 && to.getHours() === 23;
    const sameDay = from.toDateString() === to.toDateString();
    const time: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
    const span = sameDay
      ? whole
        ? from.toLocaleDateString(undefined, day)
        : `${from.toLocaleDateString(undefined, day)}, ${from.toLocaleTimeString(undefined, time)} – ${new Date(query.to).toLocaleTimeString(undefined, time)}`
      : `${from.toLocaleDateString(undefined, day)} – ${to.toLocaleDateString(undefined, day)}`;
    parts.push(span);
  }
  if (query.app) parts.push(`from ${query.app}`);
  if (query.kind) parts.push(KIND_LABELS[query.kind]);
  if (query.terms.length) parts.push(`“${query.terms.join(" ")}”`);
  return parts.join(" · ");
}

function Metadata({ entry }: { entry: Entry }) {
  const text = entry.text ?? "";
  return (
    <List.Item.Detail.Metadata>
      <List.Item.Detail.Metadata.Label title="Application" text={entry.appName ?? entry.app ?? "Unknown"} icon={entry.appIcon} />
      <List.Item.Detail.Metadata.Label title="Content type" text={KIND_LABELS[entry.kind]} />
      {entry.kind === "image" ? (
        <List.Item.Detail.Metadata.Label title="Dimensions" text={`${entry.width} × ${entry.height}`} />
      ) : (
        <>
          <List.Item.Detail.Metadata.Label title="Characters" text={String([...text].length)} />
          <List.Item.Detail.Metadata.Label title="Words" text={String(text.split(/\s+/).filter(Boolean).length)} />
        </>
      )}
      <List.Item.Detail.Metadata.Label title="Times copied" text={String(entry.timesCopied)} />
      <List.Item.Detail.Metadata.Label title="Last copied" text={formatTime(entry.at)} />
      <List.Item.Detail.Metadata.Label title="First copied" text={formatTime(entry.firstAt)} />
    </List.Item.Detail.Metadata>
  );
}

export function ClipboardHistory() {
  const [found, setFound] = useState<Found>();
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Kind | "all">("all");
  /// Answers arrive in any order; only the one to the latest question counts.
  const asked = useRef(0);
  const meaningTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  /// What is searched for: the host searches, by the words and by what it
  /// reads out of them.
  const search = (byMeaning?: true) => Memory.search({ text: query, kinds: filter === "all" ? undefined : [filter], limit: SHOWN, byMeaning });

  /// Asks for what the query means on top of its words, once typing has
  /// paused. The words were answered already and are on screen; this
  /// replaces them when it comes (with the same, if it has nothing to add).
  const askMeaning = (question: number) => {
    clearTimeout(meaningTimer.current);
    meaningTimer.current = setTimeout(async () => {
      if (question !== asked.current) return;
      const answer = await search(true).catch(() => undefined);
      if (answer && question === asked.current) setFound((last) => ({ recording: last?.recording, paused: last?.paused, meaning: last?.meaning, ...answer }));
    }, MEANING_PAUSE);
  };
  useEffect(() => () => clearTimeout(meaningTimer.current), []);

  const reload = () => {
    const question = ++asked.current;
    clearTimeout(meaningTimer.current);
    return Promise.all([search(), request<Status>("host.clipboardHistory.status")])
      .then(([answer, status]) => {
        if (question !== asked.current) return;
        setFound({ ...answer, recording: status.recording, paused: status.paused, meaning: status.meaning });
        if (status.meaning && query.trim()) askMeaning(question);
      })
      .catch((e) => {
        setFound((last) => ({ items: [], understood: { terms: [] }, relaxed: false, recording: last?.recording, paused: last?.paused }));
        showToast({ style: Toast.Style.Failure, title: "Could not load clipboard history", message: String(e) });
      });
  };
  // The host searches: the history is too long to hand over whole.
  useEffect(() => {
    reload();
  }, [query, filter]);

  const entries = found?.items;
  const read = found && (found.understood.from !== undefined || found.understood.app || found.understood.kind) ? found.understood : undefined;
  /// Entries found by meaning are in order of likeness, not of time.
  const ranked = entries?.some((entry) => entry.byMeaning) ?? false;
  const groups = useMemo(() => {
    const all = entries ?? [];
    // When the query named a time, a source or a kind, say how it was read
    // instead of dividing the results by day.
    if (read || ranked) return all.length ? [{ name: "Results", subtitle: read && (reading(read) as string | undefined), entries: all }] : [];
    const now = new Date();
    return SECTIONS.map((name) => ({ name, subtitle: undefined as string | undefined, entries: all.filter((e) => section(e, now) === name) })).filter(
      (g) => g.entries.length,
    );
  }, [found]);

  const copy = async (entry: Entry) => {
    await History.copy(entry.id);
    // What was searched for and where the entry stood is what the search is
    // measured by. Nothing waits for the host to have noted it.
    const searched = query.trim();
    if (searched) request("host.clipboardHistory.chose", { id: numberOf(entry), query: searched, rank: (entries ?? []).indexOf(entry) }).catch(() => {});
  };

  const paused = found?.paused === true;
  const off = found?.recording === false;
  const setPaused = async (value: boolean) => {
    try {
      await request("host.clipboardHistory.pause", { paused: value });
      await showToast({ style: Toast.Style.Success, title: value ? "Clipboard history is paused" : "Clipboard history resumed" });
    } catch (e) {
      await showToast({ style: Toast.Style.Failure, title: "Could not change clipboard history", message: String(e) });
    }
    reload();
  };
  const pauseAction = !off && (
    <Action
      title={paused ? "Resume Recording" : "Pause Recording"}
      icon={paused ? Icon.Play : Icon.Pause}
      shortcut={{ modifiers: ["cmd", "shift"], key: "r" }}
      onAction={() => setPaused(!paused)}
    />
  );
  /// Said in the search field, over the first section and in place of an
  /// empty list while copies are not being recorded. Not a row of its own:
  /// the first row is what Enter pastes.
  const notRecording = off ? "Recording is turned off" : paused ? "Recording is paused" : undefined;
  /// While entries are still being read, a search by meaning misses some.
  const waiting = found?.meaning?.waiting ?? 0;
  const stillReading = waiting > 0 && query.trim() ? `Finding by meaning is still reading ${waiting.toLocaleString()} ${waiting === 1 ? "entry" : "entries"}` : undefined;

  const actions = (entry: Entry) => (
    <ActionPanel>
      <ActionPanel.Section>
        <Action
          title="Paste to Active App"
          icon={Icon.Clipboard}
          onAction={async () => {
            await copy(entry);
            await closeMainWindow();
            await pasteKeystroke();
          }}
        />
        <Action
          title="Copy to Clipboard"
          icon={Icon.CopyClipboard}
          onAction={async () => {
            await copy(entry);
            await showHUD("Copied to Clipboard");
          }}
        />
        {entry.kind === "link" && <Action.OpenInBrowser url={entry.text!.trim()} shortcut={{ modifiers: ["cmd"], key: "o" }} />}
      </ActionPanel.Section>
      <ActionPanel.Section>
        <Action
          title={entry.pinned ? "Unpin Entry" : "Pin Entry"}
          icon={entry.pinned ? Icon.PinDisabled : Icon.Pin}
          shortcut={{ modifiers: ["cmd", "shift"], key: "p" }}
          onAction={async () => {
            await request("host.clipboardHistory.pin", { id: numberOf(entry), pinned: !entry.pinned });
            reload();
          }}
        />
        <Action
          title="Delete Entry"
          icon={Icon.Trash}
          style="destructive"
          shortcut={{ modifiers: ["ctrl"], key: "x" }}
          onAction={async () => {
            await request("host.clipboardHistory.delete", { id: numberOf(entry) });
            reload();
          }}
        />
        <Action
          title="Delete All Entries"
          icon={Icon.Trash}
          style="destructive"
          shortcut={{ modifiers: ["ctrl", "shift"], key: "x" }}
          onAction={async () => {
            const confirmed = await confirmAlert({
              title: "Delete all entries?",
              message: "Pinned entries are kept.",
              primaryAction: { title: "Delete All", style: "DESTRUCTIVE" },
            });
            if (!confirmed) return;
            await request("host.clipboardHistory.clear");
            reload();
          }}
        />
      </ActionPanel.Section>
      <ActionPanel.Section>{pauseAction}</ActionPanel.Section>
    </ActionPanel>
  );

  return (
    <List
      isLoading={entries === undefined}
      isShowingDetail={groups.length > 0}
      filtering={false}
      onSearchTextChange={setQuery}
      searchBarPlaceholder={`${notRecording ? `${notRecording} · ` : ""}Search, or say when and where: 昨天下午, last week, from firefox…`}
      searchBarAccessory={
        <List.Dropdown tooltip="Filter by type" value={filter} onChange={(value: string) => setFilter(value as Kind | "all")}>
          {FILTERS.map((f) => (
            <List.Dropdown.Item key={f.value} value={f.value} title={f.title} />
          ))}
        </List.Dropdown>
      }
    >
      <List.EmptyView
        icon={Icon.Clipboard}
        title={query || filter !== "all" ? "No matching entries" : "Clipboard history is empty"}
        description={
          query || filter !== "all"
            ? undefined
            : off
              ? "Recording is turned off. Turn it on in Heyo Settings, under Heyo Commands."
              : paused
                ? "Recording is paused. Run “Pause or Resume Clipboard History” to record again."
                : "Copy something and it will show up here."
        }
        actions={pauseAction ? <ActionPanel>{pauseAction}</ActionPanel> : undefined}
      />
      {groups.map((group) => (
        <List.Section
          key={group.name}
          title={group.name}
          subtitle={group === groups[0] ? [notRecording, group.subtitle, stillReading].filter(Boolean).join(" · ") || undefined : group.subtitle}
        >
          {group.entries.map((entry) => (
            <List.Item
              key={entry.id}
              id={String(numberOf(entry))}
              title={title(entry)}
              icon={icon(entry)}
              keywords={entry.app ? [entry.app, ...(entry.appName ? [entry.appName] : [])] : undefined}
              accessories={accessories(entry)}
              detail={<List.Item.Detail markdown={preview(entry)} metadata={<Metadata entry={entry} />} />}
              actions={actions(entry)}
            />
          ))}
        </List.Section>
      ))}
    </List>
  );
}
