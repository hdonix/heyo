// Writes down the arguments it was launched with.
const fs = require("node:fs");
exports.default = async (props) => {
  fs.writeFileSync(process.env.HEYO_PROBE_OUT, JSON.stringify({ arguments: props.arguments }));
};
