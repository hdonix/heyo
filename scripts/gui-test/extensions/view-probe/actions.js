// The action panel's extras: a date asked for in the panel
// (`Action.PickDate`), Heyo's own forms opened filled in
// (`Action.CreateSnippet`, `Action.CreateQuicklink`) and a file looked at in
// place (`Action.ToggleQuickLook`). $HEYO_PROBE_FILE names the file to look at.
const { Action, ActionPanel, List } = require("@raycast/api");
const React = require("react");
const { note } = require("./probe.js");

const h = React.createElement;

exports.default = () => {
  const [date, setDate] = React.useState(null);
  const file = process.env.HEYO_PROBE_FILE || __filename;
  return h(
    List,
    null,
    h(List.Item, {
      title: "Dates",
      subtitle: date ? date.toISOString() : "none picked yet",
      actions: h(
        ActionPanel,
        null,
        h(Action.PickDate, {
          title: "Pick a Date",
          onChange: (picked) => {
            setDate(picked);
            note("actions", { date: picked ? picked.toISOString() : null, type: typeof picked });
          },
        }),
        h(Action.PickDate, { title: "Pick a Day", type: Action.PickDate.Type.Date, onChange: (picked) => note("actions", { day: picked ? picked.toISOString() : null }) }),
      ),
    }),
    h(List.Item, {
      title: "Library",
      subtitle: "Create a snippet or a quicklink, filled in",
      actions: h(
        ActionPanel,
        null,
        h(Action.CreateSnippet, { snippet: { name: "Probe Signature", text: "Kind regards, the probe", keyword: ";probe" } }),
        h(Action.CreateQuicklink, { quicklink: { name: "Probe Search", link: "https://example.com/?q={Query}" } }),
      ),
    }),
    h(List.Item, {
      title: "Look",
      subtitle: file,
      quickLook: { path: file, name: "The probe's file" },
      actions: h(ActionPanel, null, h(Action.ToggleQuickLook, {}), h(Action, { title: "Nothing", onAction: () => {} })),
    }),
  );
};
