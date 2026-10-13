//! Choosing files for `Form.FilePicker`: a view pushed over the form that
//! walks the file system, a folder at a time.
//!
//! It is a view of Heyo's own rather than the desktop's file dialog: the
//! palette hides when another window takes the focus, a dialog cannot be
//! told to show hidden files or to take folders and files together, and
//! this one is driven from the keyboard like everything else here.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { useMemo, useState } from "react";

import { Action, ActionPanel, List } from "./components";
import { Icon } from "./icon";
import { clearSearchBar } from "./system";
import { t } from "../../../shared/i18n";

export interface Entry {
  name: string;
  path: string;
  directory: boolean;
}

/// What a folder holds: folders first, then files, each by name. Hidden
/// entries (a leading dot) only when asked; files only when they can be
/// chosen. A folder that cannot be read is empty.
export function listDirectory(dir: string, options: { hidden: boolean; files: boolean }): Entry[] {
  let names: fs.Dirent[];
  try {
    names = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
  const entries = names
    .filter((entry) => options.hidden || !entry.name.startsWith("."))
    .map((entry) => {
      const full = path.join(dir, entry.name);
      let directory = entry.isDirectory();
      // A link is what it points to.
      if (entry.isSymbolicLink()) {
        try {
          directory = fs.statSync(full).isDirectory();
        } catch {
          directory = false;
        }
      }
      return { name: entry.name, path: full, directory };
    })
    .filter((entry) => entry.directory || options.files);
  const order = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
  return entries.sort((a, b) => Number(b.directory) - Number(a.directory) || order.compare(a.name, b.name));
}

/// Where browsing starts: beside what is already chosen, or at home.
export function startDirectory(selected: string[]): string {
  for (const chosen of selected) {
    try {
      fs.statSync(chosen);
      return path.dirname(chosen);
    } catch {
      // Gone since it was chosen.
    }
  }
  return os.homedir();
}

/// `path` added to the chosen ones, or taken out if it is among them.
export const toggled = (chosen: string[], path: string) => (chosen.includes(path) ? chosen.filter((p) => p !== path) : [...chosen, path]);

export interface FileBrowserProps {
  multiple: boolean;
  files: boolean;
  directories: boolean;
  hidden: boolean;
  selected: string[];
  onDone(paths: string[]): void;
}

export function FileBrowser(props: FileBrowserProps) {
  const [dir, setDir] = useState(() => startDirectory(props.selected));
  const [hidden, setHidden] = useState(props.hidden);
  const [chosen, setChosen] = useState<string[]>(props.multiple ? props.selected : []);
  const entries = useMemo(() => listDirectory(dir, { hidden, files: props.files }), [dir, hidden, props.files]);
  const parent = path.dirname(dir);

  const enter = (to: string) => {
    setDir(to);
    // What filtered one folder says nothing about the next.
    void clearSearchBar();
  };
  const choose = (file: string) => (props.multiple ? setChosen((c) => toggled(c, file)) : props.onDone([file]));
  const can = (entry: Entry) => (entry.directory ? props.directories : props.files);

  const common = (
    <ActionPanel.Section>
      {props.multiple && (
        <Action
          title={chosen.length ? t("Done ({0} Selected)", chosen.length) : t("Done")}
          icon={Icon.Checkmark}
          shortcut={{ modifiers: ["cmd"], key: "d" }}
          onAction={() => props.onDone(chosen)}
        />
      )}
      {parent !== dir && <Action title={t("Go to Enclosing Folder")} icon={Icon.ArrowUp} shortcut={{ modifiers: ["cmd"], key: "arrowUp" }} onAction={() => enter(parent)} />}
      <Action
        title={hidden ? t("Hide Hidden Files") : t("Show Hidden Files")}
        icon={Icon.Eye}
        shortcut={{ modifiers: ["cmd"], key: "h" }}
        onAction={() => setHidden(!hidden)}
      />
    </ActionPanel.Section>
  );

  const here = path.basename(dir) || dir;
  /// What the search bar asks for: files, folders or either, one or several.
  const asking = () => {
    if (props.files && props.directories) return props.multiple ? t("Choose files or folders in {0}…", here) : t("Choose a file or folder in {0}…", here);
    if (props.directories) return props.multiple ? t("Choose folders in {0}…", here) : t("Choose a folder in {0}…", here);
    return props.multiple ? t("Choose files in {0}…", here) : t("Choose a file in {0}…", here);
  };
  return (
    // The footer has room for the folder's name; the section heading says where it is.
    <List navigationTitle={here} searchBarPlaceholder={asking()}>
      <List.Section title={dir} subtitle={props.multiple && chosen.length ? t("{0} selected", chosen.length) : undefined}>
        {props.directories && (
          <List.Item
            key="."
            title={t("Choose “{0}”", here)}
            subtitle={t("This folder")}
            icon={chosen.includes(dir) ? Icon.CheckCircle : Icon.Folder}
            actions={
              <ActionPanel>
                <Action title={props.multiple ? (chosen.includes(dir) ? t("Deselect Folder") : t("Select Folder")) : t("Choose Folder")} icon={Icon.Checkmark} onAction={() => choose(dir)} />
                {common}
              </ActionPanel>
            }
          />
        )}
        {parent !== dir && (
          <List.Item
            key=".."
            title=".."
            subtitle={t("Enclosing folder")}
            icon={Icon.ArrowUp}
            actions={
              <ActionPanel>
                <Action title={t("Open Folder")} icon={Icon.Folder} onAction={() => enter(parent)} />
                {common}
              </ActionPanel>
            }
          />
        )}
        {entries.map((entry) => {
          const selected = chosen.includes(entry.path);
          const pick = can(entry) && (
            <Action
              key="choose"
              title={props.multiple ? (selected ? t("Deselect") : t("Select")) : entry.directory ? t("Choose Folder") : t("Choose File")}
              icon={Icon.Checkmark}
              onAction={() => choose(entry.path)}
            />
          );
          const open = entry.directory && <Action key="open" title={t("Open Folder")} icon={Icon.Folder} onAction={() => enter(entry.path)} />;
          return (
            <List.Item
              key={entry.path}
              title={entry.name}
              icon={selected ? Icon.CheckCircle : entry.directory ? Icon.Folder : Icon.BlankDocument}
              accessories={selected ? [{ tag: t("Selected") }] : undefined}
              actions={
                <ActionPanel>
                  {/* Enter walks into a folder; a file it chooses. */}
                  {entry.directory ? [open, pick] : pick}
                  {common}
                </ActionPanel>
              }
            />
          );
        })}
      </List.Section>
    </List>
  );
}
