// Bundles the Node programs Heyo runs, each into a single CommonJS file:
// the extension runtime, the dictation, text-recognition and embedding
// helpers, and the developer tools (`heyo build`, `heyo dev`, …).
import { readFileSync } from "node:fs";
import { build } from "esbuild";

// Each file starts with a line naming the Heyo it was built for. The host
// reads it before starting the file, and refuses one from another version
// (src-tauri/src/helper.rs).
const { version } = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));

const common = {
  banner: { js: `// heyo-helper ${version}` },
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  jsx: "automatic",
  define: { "process.env.NODE_ENV": '"production"' },
  logLevel: "warning",
};

await build({ ...common, entryPoints: ["runtime/src/main.ts"], outfile: "runtime/dist/heyo-runtime.cjs" });
await build({ ...common, entryPoints: ["runtime/src/dictation.ts"], outfile: "runtime/dist/heyo-dictation.cjs" });
await build({ ...common, entryPoints: ["runtime/src/ocr.ts"], outfile: "runtime/dist/heyo-ocr.cjs" });
await build({ ...common, entryPoints: ["runtime/src/dev/main.ts"], outfile: "runtime/dist/heyo-dev.cjs" });
await build({ ...common, entryPoints: ["runtime/src/embed.ts"], outfile: "runtime/dist/heyo-embed.cjs" });
