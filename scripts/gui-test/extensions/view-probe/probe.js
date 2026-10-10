// What the commands share: writing down what happened, in
// probe-<name>.json beside $HEYO_PROBE_OUT (or in the temporary directory:
// the menu bar command runs again whenever Heyo starts, test or no test).
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

exports.note = (name, value) => {
  const file = path.join(process.env.HEYO_PROBE_OUT ? path.dirname(process.env.HEYO_PROBE_OUT) : os.tmpdir(), `probe-${name}.json`);
  fs.writeFileSync(file, JSON.stringify(value, null, 1));
};
