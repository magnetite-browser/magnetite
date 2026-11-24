// The InfoPlist.strings of one translation for the macOS app
// (scripts/build.sh puts it into <language>.lproj): the texts macOS shows
// for Magnetite, such as the reasons in its permission prompts, from
// crates/ui/locales/<language>/macos.json. English is packaging/macos/Info.plist
// itself; a text a translation leaves out stays English.
//
//   node tools/info-plist-strings.mjs <language>

import { readFileSync } from "node:fs";

const KEYS = {
  copyright: "NSHumanReadableCopyright",
  camera: "NSCameraUsageDescription",
  microphone: "NSMicrophoneUsageDescription",
  bluetooth: "NSBluetoothAlwaysUsageDescription",
  passkeys: "NSWebBrowserPublicKeyCredentialUsageDescription",
};

const tag = process.argv[2];
if (!tag) throw new Error("usage: info-plist-strings.mjs <language>");
const file = new URL(`../crates/ui/locales/${tag}/macos.json`, import.meta.url);
let texts = {};
try {
  texts = JSON.parse(readFileSync(file, "utf8")).info_plist ?? {};
} catch (e) {
  if (e.code !== "ENOENT") throw e;
}
const quoted = (s) => `"${s.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
const lines = ['CFBundleName = "Magnetite";', 'CFBundleDisplayName = "Magnetite";'];
for (const [name, key] of Object.entries(KEYS)) if (texts[name]) lines.push(`${key} = ${quoted(texts[name])};`);
process.stdout.write(lines.join("\n") + "\n");
