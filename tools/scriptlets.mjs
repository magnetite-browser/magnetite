// Write the code of uBlock Origin's scriptlets as the resource list
// adblock-rust reads (`Filters::compile` in crates/engine/src/blocker.rs).
// scripts/scriptlets.sh runs this on a checkout of uBlock Origin.
//
//   node tools/scriptlets.mjs <uBlock checkout> <scriptlets.json>
//
// Each scriptlet is one function, kept as its source text. One that uBlock
// Origin reserves for trusted lists gets permission 1, which only a list
// Magnetite trusts has.
import { writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [checkout, out] = process.argv.slice(2);
if (!checkout || !out) {
  console.error("usage: node tools/scriptlets.mjs <uBlock checkout> <scriptlets.json>");
  process.exit(2);
}
const { builtinScriptlets } = await import(pathToFileURL(resolve(checkout, "src/js/resources/scriptlets.js")));
const resources = builtinScriptlets.map((scriptlet) => ({
  name: scriptlet.name,
  aliases: scriptlet.aliases ?? [],
  // `.fn` names are helpers other scriptlets depend on, never run by a rule.
  kind: { mime: scriptlet.name.endsWith(".fn") ? "fn/javascript" : "application/javascript" },
  content: Buffer.from(scriptlet.fn.toString()).toString("base64"),
  dependencies: scriptlet.dependencies ?? [],
  ...(scriptlet.requiresTrust ? { permission: 1 } : {}),
}));
if (resources.length === 0) {
  console.error("no scriptlets in the checkout");
  process.exit(1);
}
writeFileSync(out, JSON.stringify(resources));
