// Stands in for Heyo's helpers (heyo-ocr.cjs, heyo-embed.cjs) when a download is tried in the sandbox: it fetches
// nothing. Asked to install, it reports progress for a few seconds, as an
// installer does, leaves the empty files Heyo takes for the thing being
// there, and says it is installed; with HEYO_INSTALL_STUB=fail it says
// what went wrong instead. Asked anything else, it ends without a word.
//
//   HEYO_OCR=…/install-stub.cjs HEYO_EMBED=… heyo
//
// (Dictation's models are fetched by Heyo itself: HEYO_DICTATION_MIRROR
// names a server to fetch them from instead.)
const fs = require("node:fs");
const path = require("node:path");

const say = (message) => process.stdout.write(JSON.stringify(message) + "\n");
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Which helper this is run as: by the directories Heyo names for it.
const places = {
  embed: [process.env.HEYO_EMBED_DIR, ["node_modules/onnxruntime-node/", "model/model_quantized.onnx", "model/tokenizer.json"]],
  ocr: [process.env.HEYO_OCR_DIR, ["node_modules/@gutenye/ocr-node/"]],
};
const kind = places.embed[0] ? "embed" : "ocr";

(async () => {
  if (process.argv[2] !== "install") return;
  const [dir, files] = places[kind];
  const total = 4_000_000;
  for (const step of ["Engine", "Model"]) {
    for (let received = 0; received <= total; received += total / 4) {
      say({ type: "progress", step, received, total });
      await wait(Number(process.env.HEYO_INSTALL_STUB_PACE ?? 400));
    }
  }
  if (process.env.HEYO_INSTALL_STUB === "fail") {
    say({ type: "error", message: "The sandbox has nothing to download from" });
    return;
  }
  for (const file of files) {
    const target = path.join(dir, file);
    if (file.endsWith("/")) fs.mkdirSync(target, { recursive: true });
    else {
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, "");
    }
  }
  say({ type: "installed" });
})();
