// A list whose search bar dropdown keeps its selection (`storeValue`): what
// it reports through `onChange` is written down each time.
const { Action, ActionPanel, List } = require("@raycast/api");
const React = require("react");
const { note } = require("./probe.js");

const h = React.createElement;

exports.default = () => {
  const [kind, setKind] = React.useState("");
  const onChange = (value) => {
    setKind(value);
    note("stored-list", { dropdown: value });
  };
  return h(
    List,
    {
      searchBarPlaceholder: "The dropdown on the right keeps its choice",
      searchBarAccessory: h(
        List.Dropdown,
        { id: "kind", tooltip: "Kind", storeValue: true, onChange },
        h(List.Dropdown.Item, { value: "apples", title: "Apples" }),
        h(List.Dropdown.Item, { value: "pears", title: "Pears" }),
        h(List.Dropdown.Item, { value: "plums", title: "Plums" }),
      ),
    },
    h(List.Item, {
      title: `Showing ${kind || "nothing yet"}`,
      actions: h(ActionPanel, null, h(Action, { title: "Nothing", onAction: () => {} })),
    }),
  );
};
