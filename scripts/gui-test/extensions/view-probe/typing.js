// A search bar the extension controls, answered slowly: each change is
// taken up a quarter of a second later, the way a slow extension would.
// What it holds is written down; the first row shows it; a pushed view
// comes back to it.
const { Action, ActionPanel, Detail, List, useNavigation } = require("@raycast/api");
const React = require("react");
const { note } = require("./probe.js");

const h = React.createElement;

function Pushed() {
  const { pop } = useNavigation();
  return h(Detail, { markdown: "# Pushed\n\nEsc goes back.", actions: h(ActionPanel, null, h(Action, { title: "Back", onAction: pop })) });
}

exports.default = () => {
  const [text, setText] = React.useState("");
  const { push } = useNavigation();
  const onSearchTextChange = (typed) => {
    setTimeout(() => {
      setText(typed);
      note("typing", { text: typed });
    }, 250);
  };
  return h(
    List,
    { searchText: text, onSearchTextChange, filtering: false, searchBarPlaceholder: "Type fast" },
    ["one", "two", "three"].map((name, i) =>
      h(List.Item, {
        key: name,
        title: i === 0 ? `Extension holds: “${text}”` : name,
        actions: h(ActionPanel, null, h(Action, { title: "Push a View", onAction: () => push(h(Pushed)) })),
      }),
    ),
  );
};
