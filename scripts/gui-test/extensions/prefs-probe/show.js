// Writes down the preferences the runtime hands this command: what the
// settings window saved is what a command gets.
const fs = require("node:fs");
const path = require("node:path");
const { getPreferenceValues } = require("@heyo/api");

exports.default = async () => {
  fs.writeFileSync(path.join(path.dirname(process.env.HEYO_PROBE_OUT), "probe-prefs.json"), JSON.stringify(getPreferenceValues(), null, 1));
};
