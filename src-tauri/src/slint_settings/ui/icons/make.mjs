// Writes the symbols the settings window draws: every Lucide icon the web
// interface could draw for a Raycast icon name (lucide-react, ISC: see
// LICENSE here), as two tables the host reads (src/slint_settings/icons.rs):
//
//   lucide.tsv    name <tab> the shapes of its SVG
//   raycast.tsv   a Raycast icon name <tab> the Lucide name it is drawn as,
//                 where the two differ (src/raycast/iconAliases.ts)
//
// and the two the `.slint` files name themselves, search.svg and x.svg.
// Run from the repository after `pnpm install`, and again when lucide-react
// or iconAliases.ts changes:
//
//   node src-tauri/src/slint_settings/ui/icons/make.mjs
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../../../..");
const lucide = join(root, "node_modules/lucide-react/dist/esm/icons");

const shapes = (node) =>
  node
    .map(([tag, attributes]) => {
      const written = Object.entries(attributes)
        .filter(([key]) => key !== "key")
        .map(([key, value]) => `${key}="${value}"`)
        .join(" ");
      return `<${tag} ${written}/>`;
    })
    .join("");

export async function tables() {
  const icons = new Map();
  for (const file of readdirSync(lucide).filter((name) => name.endsWith(".mjs")).sort()) {
    const { __iconData } = await import(pathToFileURL(join(lucide, file)).href);
    if (__iconData && !icons.has(__iconData.name)) icons.set(__iconData.name, shapes(__iconData.node));
  }
  // The web interface finds an icon by its name in PascalCase.
  const pascal = (name) => name.split("-").map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join("");
  const byPascal = new Map([...icons.keys()].map((name) => [pascal(name), name]));
  const aliases = [];
  for (const [, raycast, target] of readFileSync(join(root, "src/raycast/iconAliases.ts"), "utf8").matchAll(/^\s*"?([\w-]+)"?: "(\w+)",$/gm)) {
    const name = byPascal.get(target);
    if (name) aliases.push(`${raycast}\t${name}`);
  }
  return {
    lucide: [...icons].map(([name, drawn]) => `${name}\t${drawn}`).join("\n") + "\n",
    raycast: aliases.join("\n") + "\n",
    svg: (name) => `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${icons.get(name)}</svg>\n`,
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const made = await tables();
  writeFileSync(join(here, "lucide.tsv"), made.lucide);
  writeFileSync(join(here, "raycast.tsv"), made.raycast);
  for (const name of ["search", "x"]) writeFileSync(join(here, `${name}.svg`), made.svg(name));
}
