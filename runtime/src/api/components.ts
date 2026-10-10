//! Raycast UI components. Each renders a host element named after the
//! component; element-valued props (`actions`, `detail`, …) are lifted into
//! `slot` children so the reconciler renders them like any other subtree.

import { Children, createElement, isValidElement, useEffect, useRef, useState, type ReactNode } from "react";

import { QuicklinkForm, SnippetForm } from "../builtins/library";
import { FileBrowser } from "./files";
import { Icon } from "./icon";
import { useNavigation } from "./navigation";
import { QUICK_LOOK_SHORTCUT, quickLook } from "./quicklook";
import { dropdownKey, fieldKey, keep, store, stored, storeSubmitted } from "./stored";
import { Clipboard, closeMainWindow, open, showHUD, showInFinder, trash } from "./system";

const h = createElement;
type Props = Record<string, any>;

function host(type: string, slots: string[] = []) {
  const component = ({ children, ...props }: Props) => {
    const lifted = slots.flatMap((name) => {
      const value = props[name];
      if (value == null || typeof value !== "object") return [];
      delete props[name];
      return [h("slot", { key: `slot:${name}`, name }, value)];
    });
    return h(type, props, ...lifted, children);
  };
  component.displayName = type;
  return component;
}

/// How many changes sent are remembered, waiting to be echoed.
const SENT_LIMIT = 64;

/// The webview owns what is being typed and numbers each change it sends
/// (see src/raycast/typing.ts). This answers with a number, as `ack`, beside
/// the `value` the extension shows: that is how the webview tells an answer
/// to the latest keystroke from one to an earlier keystroke.
///
/// The number is not simply the last change handled: an extension that
/// takes its time (a value set after a fetch, a timeout) answers a keystroke
/// after later ones were handled, and its answer would pass as the latest.
/// So the changes sent are kept, and a value that is one of them is the
/// echo of that change and carries its number; any other value is the
/// extension's own doing, and is as new as the last change handled.
function useAck(props: Props, handler: string, value: string): Props {
  const [latest, setLatest] = useState(0);
  const sent = useRef<{ seq: number; text: unknown }[]>([]);
  const change = props[handler];
  if (typeof change !== "function") return props;
  const shown = props[value];
  const echoed = sent.current.findLast((one) => one.text === shown);
  if (echoed) {
    // What was sent before it has been answered, or passed over. It stays
    // itself: shown again, it is the same echo.
    sent.current = sent.current.filter((one) => one.seq >= echoed.seq);
  }
  return {
    ...props,
    ack: echoed?.seq ?? latest,
    [handler]: (text: unknown, seq?: unknown) => {
      if (typeof seq === "number") {
        sent.current = [...sent.current.slice(1 - SENT_LIMIT), { seq, text }];
        setLatest(seq);
      }
      return change(text);
    },
  };
}

/// Reading or writing what `storeValue` keeps. A file that cannot be read
/// or written costs the convenience, never the command.
function keeping<T>(work: () => T): T | undefined {
  try {
    return work();
  } catch (e) {
    console.error(e);
    return undefined;
  }
}

const remembered = (key: string) => keeping(() => stored(key));

/// The `value`s of the `Dropdown.Item` descendants that can be seen from here.
function optionValues(children: ReactNode): string[] {
  return Children.toArray(children).flatMap((child): string[] => {
    if (!isValidElement<Props>(child)) return [];
    if (child.props.value !== undefined && child.props.children === undefined) return [child.props.value];
    return optionValues(child.props.children);
  });
}

/// A search bar dropdown (`List.Dropdown`, `Grid.Dropdown`).
function searchBarDropdown(type: string) {
  const Dropdown = ({ children, ...props }: Props) => {
    const key = dropdownKey(props.id);
    // `storeValue`: the last selection, kept when it is made.
    const [kept] = useState(() => (props.storeValue ? remembered(key) : undefined));
    const options = optionValues(children);
    // A kept value whose item is gone selects nothing.
    const usable = typeof kept === "string" && (!options.length || options.includes(kept)) ? kept : undefined;
    const initial = usable ?? props.defaultValue ?? options[0];
    // Raycast reports the initial selection of an uncontrolled dropdown.
    useEffect(() => {
      if (props.value === undefined) props.onChange?.(initial);
    }, []);
    const onChange =
      props.storeValue || props.onChange
        ? (value: string) => {
            if (props.storeValue) keeping(() => store({ [key]: value }));
            return props.onChange?.(value);
          }
        : undefined;
    return h(type, { ...props, defaultValue: initial, onChange }, children);
  };
  return Object.assign(Dropdown, {
    Item: host(`${type}.Item`),
    Section: host(`${type}.Section`),
  });
}

// --- Metadata ------------------------------------------------------------

const TagList = Object.assign(host("Metadata.TagList"), { Item: host("Metadata.TagList.Item") });

function metadata(type: string) {
  return Object.assign(host(type), {
    Label: host("Metadata.Label"),
    Link: host("Metadata.Link"),
    Separator: host("Metadata.Separator"),
    TagList,
  });
}

// --- List ----------------------------------------------------------------

const ListItemDetail = Object.assign(host("List.Item.Detail", ["metadata"]), {
  Metadata: metadata("Metadata"),
});

/// The names older versions of the API had for props, given their present ones.
function renamed(component: ReturnType<typeof host>, rename: (props: Props) => Props) {
  const wrapped = (props: Props) => h(component, rename(props));
  wrapped.displayName = component.displayName;
  return wrapped;
}

/// `enableFiltering` became `filtering`.
const withFiltering = ({ enableFiltering, ...props }: Props) => (enableFiltering === undefined ? props : { filtering: enableFiltering, ...props });

/// `accessoryTitle` and `accessoryIcon` became entries of `accessories`.
const withAccessories = ({ accessoryTitle, accessoryIcon, ...props }: Props) =>
  accessoryTitle == null && accessoryIcon == null
    ? props
    : { ...props, accessories: [...(props.accessories ?? []), { text: accessoryTitle ?? undefined, icon: accessoryIcon ?? undefined }] };

/// `List` and `Grid`: a view with a search bar.
function searchable(type: string) {
  const view = renamed(host(type, ["searchBarAccessory", "actions"]), withFiltering);
  const Searchable = (props: Props) => h(view, useAck(props, "onSearchTextChange", "searchText"));
  Searchable.displayName = type;
  return Searchable;
}

export const List = Object.assign(searchable("List"), {
  Item: Object.assign(renamed(host("List.Item", ["actions", "detail"]), withAccessories), { Detail: ListItemDetail }),
  Section: host("List.Section"),
  EmptyView: host("List.EmptyView", ["actions"]),
  Dropdown: searchBarDropdown("Dropdown"),
});

// --- Grid ----------------------------------------------------------------

export const Grid = Object.assign(searchable("Grid"), {
  Item: host("Grid.Item", ["actions"]),
  Section: host("Grid.Section"),
  EmptyView: host("Grid.EmptyView", ["actions"]),
  Dropdown: searchBarDropdown("Dropdown"),
  Inset: { Small: "small", Medium: "medium", Large: "large", Zero: "zero" },
  Fit: { Contain: "contain", Fill: "fill" },
  ItemSize: { Small: "small", Medium: "medium", Large: "large" },
});

// --- Detail --------------------------------------------------------------

export const Detail = Object.assign(host("Detail", ["metadata", "actions"]), {
  Metadata: metadata("Metadata"),
});

// --- Form ----------------------------------------------------------------

/// `storeValue`: the field starts from the value it had when the form was
/// last submitted (`Action.SubmitForm` keeps it), ahead of `defaultValue`.
/// The extension hears of it through `onChange`, as it would of the user
/// putting it there; a field it controls can show it no other way.
function useStoredValue(props: Props): Props {
  const keeps = !!props.storeValue && typeof props.id === "string";
  const [kept] = useState(() => (keeps ? remembered(fieldKey(props.id)) : undefined));
  useEffect(() => (keeps ? keep(props.id) : undefined), [keeps, props.id]);
  useEffect(() => {
    if (kept !== undefined) props.onChange?.(kept);
  }, []);
  return kept === undefined ? props : { ...props, defaultValue: kept };
}

/// A form field. `text` is one that is typed into.
function field(type: string, text = false) {
  const element = host(type);
  const Field = (given: Props) => {
    const props = useStoredValue(given);
    const typed = useAck(props, "onChange", "value");
    return h(element, text ? typed : props);
  };
  Field.displayName = type;
  return Field;
}

const isFullDay = (value: Date | null | undefined) =>
  value != null && value.getHours() === 0 && value.getMinutes() === 0 && value.getSeconds() === 0 && value.getMilliseconds() === 0;

/// `Form.FilePicker`. The webview shows what is chosen; choosing happens in
/// a view pushed over the form (`FileBrowser`), and what was chosen there
/// comes back to the webview's field as `picked`.
function FilePicker(given: Props) {
  const props = useStoredValue(given);
  const navigation = useNavigation();
  const [picked, setPicked] = useState<{ seq: number; paths: string[] }>();
  const browse = (selected: unknown) =>
    navigation.push(
      h(FileBrowser, {
        multiple: !!props.allowMultipleSelection,
        files: props.canChooseFiles !== false,
        directories: !!props.canChooseDirectories,
        hidden: !!props.showHiddenFiles,
        selected: Array.isArray(selected) ? selected.map(String) : [],
        onDone: (paths: string[]) => {
          setPicked((last) => ({ seq: (last?.seq ?? 0) + 1, paths }));
          navigation.pop();
        },
      }),
    );
  return h("Form.FilePicker", { ...props, picked, browse });
}

export const Form = Object.assign(host("Form", ["actions", "searchBarAccessory"]), {
  TextField: field("Form.TextField", true),
  PasswordField: field("Form.PasswordField", true),
  TextArea: field("Form.TextArea", true),
  Checkbox: field("Form.Checkbox"),
  DatePicker: Object.assign(field("Form.DatePicker"), {
    Type: { Date: "date", DateTime: "date_time" },
    isFullDay,
  }),
  Dropdown: Object.assign(field("Form.Dropdown"), {
    Item: host("Form.Dropdown.Item"),
    Section: host("Form.Dropdown.Section"),
  }),
  TagPicker: Object.assign(field("Form.TagPicker"), { Item: host("Form.TagPicker.Item") }),
  FilePicker,
  Description: host("Form.Description"),
  Separator: host("Form.Separator"),
  LinkAccessory: host("Form.LinkAccessory"),
});

// --- MenuBarExtra (rendered by background.ts into a tray menu)

export const MenuBarExtra = Object.assign(host("MenuBarExtra"), {
  Item: host("MenuBarExtra.Item"),
  Section: host("MenuBarExtra.Section"),
  Separator: host("MenuBarExtra.Separator"),
  Submenu: host("MenuBarExtra.Submenu"),
});

// --- Actions -------------------------------------------------------------

export const ActionPanel = Object.assign(host("ActionPanel"), {
  // The name actions had before `Action` (still in extensions built then).
  Item: (props: Props) => h(Action, props),
  Section: host("ActionPanel.Section"),
  Submenu: host("ActionPanel.Submenu"),
});

function action(defaults: Props, perform: (props: Props) => unknown) {
  return ({ title, icon, ...props }: Props) =>
    h("Action", {
      ...props,
      title: title ?? defaults.title,
      icon: icon ?? defaults.icon,
      onAction: () => perform(props),
    });
}

function Push({ target, onPush, onPop, ...props }: Props) {
  const { push } = useNavigation();
  return h("Action", {
    ...props,
    onAction: () => {
      push(target, onPop);
      onPush?.();
    },
  });
}

/// Opens Heyo's own Create Snippet form, filled in from `snippet`.
function CreateSnippet({ snippet, title, icon, ...props }: Props) {
  const { push, pop } = useNavigation();
  return h("Action", {
    ...props,
    title: title ?? "Create Snippet",
    icon: icon ?? Icon.Snippets,
    onAction: () => push(h(SnippetForm, { draft: { name: snippet?.name, text: snippet?.text, keyword: snippet?.keyword }, onSaved: pop })),
  });
}

/// Opens Heyo's own Create Quicklink form, filled in from `quicklink`.
/// (A quicklink here opens with the default application: `application` and
/// `icon` have nowhere to go.)
function CreateQuicklink({ quicklink, title, icon, ...props }: Props) {
  const { push, pop } = useNavigation();
  return h("Action", {
    ...props,
    title: title ?? "Create Quicklink",
    icon: icon ?? Icon.Link,
    onAction: () => push(h(QuicklinkForm, { draft: { name: quicklink?.name, link: quicklink?.link }, onSaved: pop })),
  });
}

/// The webview runs this with the selected item's `quickLook` (it knows the
/// selection), and the file is shown in a view pushed over the list.
function ToggleQuickLook({ title, icon, shortcut, ...props }: Props) {
  const navigation = useNavigation();
  const key = shortcut ?? QUICK_LOOK_SHORTCUT;
  return h("Action", {
    ...props,
    title: title ?? "Quick Look",
    icon: icon ?? Icon.Eye,
    shortcut: key,
    preview: true,
    onAction: (target: { path?: unknown; name?: string | null } | undefined) => quickLook(target, navigation, key),
  });
}

/// The webview asks for the date in the action panel, then calls `onChange`.
function PickDate({ title, icon, type, min, max, ...props }: Props) {
  return h("Action", {
    ...props,
    title: title ?? "Set Date…",
    icon: icon ?? Icon.Calendar,
    pickDate: { type: type ?? "date_time", min, max },
  });
}

export const Action = Object.assign((props: Props) => h("Action", props), {
  Style: { Regular: "regular", Destructive: "destructive" },
  CopyToClipboard: action({ title: "Copy to Clipboard", icon: Icon.CopyClipboard }, async (p) => {
    await Clipboard.copy(p.content, { concealed: p.concealed, transient: p.transient });
    await showHUD("Copied to Clipboard");
    p.onCopy?.(p.content);
  }),
  Paste: action({ title: "Paste", icon: Icon.Clipboard }, async (p) => {
    await Clipboard.paste(p.content);
    p.onPaste?.(p.content);
  }),
  OpenInBrowser: action({ title: "Open in Browser", icon: Icon.Globe }, async (p) => {
    await open(p.url);
    await closeMainWindow();
    p.onOpen?.(p.url);
  }),
  Open: action({ title: "Open", icon: Icon.Finder }, async (p) => {
    await open(p.target, p.application);
    await closeMainWindow();
    p.onOpen?.(p.target);
  }),
  OpenWith: action({ title: "Open With", icon: Icon.Upload }, async (p) => {
    await open(p.path);
    await closeMainWindow();
    p.onOpen?.(p.path);
  }),
  ShowInFinder: action({ title: "Show in Files", icon: Icon.Finder }, async (p) => {
    await showInFinder(p.path);
    await closeMainWindow();
    p.onShow?.(p.path);
  }),
  Trash: action({ title: "Move to Trash", icon: Icon.Trash }, async (p) => {
    await trash(p.paths);
    p.onTrash?.(p.paths);
  }),
  Push,
  SubmitForm: ({ title, icon, onSubmit, ...props }: Props) =>
    h("Action", {
      ...props,
      title: title ?? "Submit Form",
      icon: icon ?? Icon.Checkmark,
      submit: true,
      onSubmit: (values: unknown) => {
        // What the fields with `storeValue` held, for next time.
        keeping(() => storeSubmitted(values));
        return onSubmit?.(values);
      },
    }),
  CreateSnippet,
  CreateQuicklink,
  ToggleQuickLook,
  PickDate: Object.assign(PickDate, {
    Type: { Date: "date", DateTime: "date_time" },
    isFullDay,
  }),
});
