// A menu bar command that counts the clicks on its item. Staying mounted
// (background.ts) is what lets the count go up: a command loaded afresh
// for each click would start from nought every time.
const { MenuBarExtra } = require("@raycast/api");
const React = require("react");
const { note } = require("./probe.js");

const h = React.createElement;

exports.default = () => {
  const [clicks, setClicks] = React.useState(0);
  React.useEffect(() => note("counter", { clicks }), [clicks]);
  return h(
    MenuBarExtra,
    { title: `Clicks: ${clicks}`, tooltip: "The probe's counter" },
    h(MenuBarExtra.Item, { title: `Clicked ${clicks} times`, onAction: () => setClicks((n) => n + 1) }),
    h(MenuBarExtra.Item, { title: "Nothing", onAction: () => {} }),
  );
};
