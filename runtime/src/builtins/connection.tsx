//! An item a connector synced, previewed from the root search: what it
//! says, with who it is from and to, when, and where it is kept. The host
//! starts it with the item and its account's name (`preview_connection`);
//! the root search does not list it.

import { Action, ActionPanel, Detail } from "../api/components";
import { Icon } from "../api/icon";
import { Clipboard, showHUD } from "../api/system";
import type { ConnectionItem } from "../../../shared/bindings/ConnectionItem";
import { copyFromTitle, kindTitle, markdown, people, sender, span } from "../../../shared/connections";
import { t } from "../../../shared/i18n";

export function ConnectionItemPreview(props: { arguments?: { item?: string; account?: string } }) {
  const item = JSON.parse(props.arguments?.item ?? "null") as ConnectionItem | null;
  if (!item) return <Detail markdown="" />;
  const copy = async (text: string) => {
    await Clipboard.copy(text);
    await showHUD(t("Copied to Clipboard"));
  };
  const from = people(item, "from", "organizer");
  const to = people(item, "to", "attendee");
  const cc = people(item, "cc");
  const address = sender(item)?.address;
  const attachments = item.attachments ?? [];
  return (
    <Detail
      navigationTitle={item.title}
      markdown={markdown(item)}
      // What a service sent: no raw HTML, no images fetched from wherever it points.
      {...{ untrusted: true }}
      metadata={
        <Detail.Metadata>
          <Detail.Metadata.Label title={t("Account")} text={props.arguments?.account ?? ""} icon={Icon.Plug} />
          <Detail.Metadata.Label title={t("Kind")} text={kindTitle(item.kind)} />
          {from && <Detail.Metadata.Label title={item.kind === "event" ? t("Organizer") : t("From")} text={from} />}
          {to && <Detail.Metadata.Label title={item.kind === "event" ? t("Attendees") : t("To")} text={to} />}
          {cc && <Detail.Metadata.Label title={t("Cc")} text={cc} />}
          <Detail.Metadata.Label title={t("When")} text={span(item)} />
          {item.location && <Detail.Metadata.Label title={t("Location")} text={item.location} />}
          {item.folder && <Detail.Metadata.Label title={t("Folder")} text={item.folder} />}
          {attachments.length > 0 && <Detail.Metadata.Label title={t("Attachments")} text={attachments.map((file) => file.name).join(", ")} />}
          {item.url && <Detail.Metadata.Link title={t("Link")} target={item.url} text={item.url} />}
        </Detail.Metadata>
      }
      actions={
        <ActionPanel>
          {item.url && <Action.OpenInBrowser url={item.url} />}
          {item.text && <Action title={t("Copy Body")} icon={Icon.CopyClipboard} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} onAction={() => copy(item.text ?? "")} />}
          {address && <Action title={copyFromTitle(item)} icon={Icon.AtSymbol} shortcut={{ modifiers: ["cmd", "shift"], key: "a" }} onAction={() => copy(address)} />}
          <Action title={t("Copy Title")} icon={Icon.CopyClipboard} onAction={() => copy(item.title)} />
        </ActionPanel>
      }
    />
  );
}
