// `Windows` (permission "windows", which the script allows beforehand): the
// windows there are, the one the user works in, and that one put into a
// layout and then exactly somewhere.
const { Windows } = require("@heyo/api");
const { probe, sleep } = require("./probe.js");

const boundsOf = async (id) => (await Windows.list()).find((w) => w.id === id)?.bounds;

exports.default = () =>
  probe("windows", {
    monitors: () => Windows.monitors(),
    list: async () => (await Windows.list()).map(({ id, title, class: name, active, monitor }) => ({ id, title, class: name, active, monitor })),
    active: () => Windows.active(),
    arranged: async (out) => {
      await Windows.arrange(out.active.id, "left-half");
      await sleep(600);
      return boundsOf(out.active.id);
    },
    fraction: async (out) => {
      await Windows.arrange(out.active.id, { fraction: [6, 0, 6, 6] });
      await sleep(600);
      return boundsOf(out.active.id);
    },
    moved: async (out) => {
      await Windows.setBounds(out.active.id, { x: 300, y: 200, width: 640, height: 480 });
      await sleep(600);
      return boundsOf(out.active.id);
    },
    // What cannot be: refused with "invalid", nothing moved.
    badLayout: (out) => Windows.arrange(out.active.id, "sideways"),
    badFraction: (out) => Windows.arrange(out.active.id, { fraction: [8, 0, 6, 12] }),
    activated: async (out) => {
      await Windows.activate(out.active.id);
      await sleep(400);
      return (await Windows.active())?.id === out.active.id;
    },
  });
