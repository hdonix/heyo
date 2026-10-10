// What the heyo-* commands share: each calls a part of `@heyo/api` and
// writes down what came back, a failure as its code and message, in
// probe-<name>.json beside $HEYO_PROBE_OUT.
const fs = require("node:fs");
const path = require("node:path");

exports.probe = async (name, steps) => {
  const out = {};
  for (const [step, run] of Object.entries(steps)) {
    try {
      const result = await run(out);
      out[step] = result === undefined ? null : result;
    } catch (e) {
      out[step] = { error: e && e.code ? e.code : "thrown", message: String(e && e.message ? e.message : e) };
    }
  }
  const file = path.join(path.dirname(process.env.HEYO_PROBE_OUT), `probe-${name}.json`);
  fs.writeFileSync(file, JSON.stringify(out, null, 1));
};

exports.sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
