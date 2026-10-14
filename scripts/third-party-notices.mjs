#!/usr/bin/env node
// The third-party notices the app shows (ADR 0087): every library built into it, with
// the licence texts each one ships. `npm run dev` and `npm run build` write them into
// the web assets (apps/desktop/public/third-party-notices.json, not committed); Settings
// → About reads them from there.
//
// - Rust crates: those the desktop app links on macOS (normal dependencies, followed from
//   the `desktop` crate; build and dev dependencies aren't shipped). `cargo deny check`
//   holds them to the licence policy (deny.toml).
// - JavaScript packages: those package-lock.json doesn't mark as development only; their
//   licences are checked against JS_ALLOWED here.
// - The Rust standard library, from the notices the Rust toolchain ships for it.
// - Native libraries (ADR 0086): versions from scripts/build-native-macos.sh, licence
//   texts from apps/desktop/src-tauri/licenses, which the app bundle also carries.
//
// A crate or package that ships no licence file gets the standard text of the licence
// it declares (scripts/licence-texts), MIT with its authors' copyright line. The script
// fails if something has no text, or a JavaScript package's licence isn't allowed.
//
// Usage: node scripts/third-party-notices.mjs <output.json>

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const DESKTOP = join(ROOT, "apps/desktop");
const TARGET = "aarch64-apple-darwin";

/** Licences a JavaScript package the app ships may have (CLAUDE.md §17). */
const JS_ALLOWED = new Set(["MIT", "ISC", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "0BSD", "Zlib", "OFL-1.1"]);

/** Crates that ship no licence file but share their repository with one that does. */
const LICENCE_FROM = { "alloc-stdlib": "alloc-no-stdlib" };

/** Standard texts, by SPDX id, for crates without a licence file; preferred in this order. */
const STANDARD = ["MIT", "MPL-2.0"];

const LICENCE_FILE = /^(licen[cs]e|copying|copyright|notice)(?!.*\.spdx$)/i;

function fail(message) {
  console.error(`third-party-notices: ${message}`);
  process.exit(1);
}

/** Texts, keyed by a short hash, so the many identical Apache and MIT texts appear once. */
const texts = new Map();
function addText(text) {
  const normalised = text.replace(/\r\n/g, "\n").trim();
  const id = createHash("sha256").update(normalised).digest("hex").slice(0, 12);
  texts.set(id, normalised);
  return id;
}

function licenceFiles(dir) {
  return readdirSync(dir)
    .filter((name) => LICENCE_FILE.test(name) && statSync(join(dir, name)).isFile())
    .sort()
    .map((name) => addText(readFileSync(join(dir, name), "utf8")));
}

function standardText(licence, authors, name) {
  const ids = licence.split(/\s+(?:OR|AND)\s+|\/|[()]/).map((s) => s.trim());
  const id = STANDARD.find((s) => ids.includes(s));
  if (!id) return null;
  const body = readFileSync(join(ROOT, "scripts/licence-texts", `${id}.txt`), "utf8");
  if (id !== "MIT") return addText(body);
  const holders = authors.length ? authors.map((a) => a.replace(/\s*<[^>]*>/, "")).join(", ") : `the ${name} authors`;
  return addText(`Copyright (c) ${holders}\n\n${body}`);
}

function rustCrates() {
  const meta = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET], {
      cwd: ROOT,
      maxBuffer: 256 * 1024 * 1024,
    }),
  );
  const packages = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const desktop = meta.packages.find((p) => p.name === "desktop" && !p.source);
  if (!desktop) fail("no desktop crate in the workspace");
  const shipped = new Set();
  const stack = [desktop.id];
  while (stack.length) {
    const id = stack.pop();
    if (shipped.has(id)) continue;
    shipped.add(id);
    for (const dep of nodes.get(id).deps) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  const byName = new Map(meta.packages.map((p) => [p.name, p]));
  return [...shipped]
    .map((id) => packages.get(id))
    .filter((p) => p.source)
    .map((p) => {
      const from = byName.get(LICENCE_FROM[p.name]) ?? p;
      let ids = licenceFiles(dirname(from.manifest_path));
      if (!ids.length) ids = [standardText(p.license ?? "", p.authors, p.name)].filter(Boolean);
      if (!ids.length) fail(`${p.name} ${p.version} ships no licence text and declares "${p.license}"`);
      return {
        name: p.name,
        version: p.version,
        licence: p.license ?? "see text",
        source: p.repository ?? `https://crates.io/crates/${p.name}`,
        texts: ids,
      };
    });
}

const decode = (html) =>
  html
    .replace(/<[^>]*>/g, "")
    .replace(/&#(\d+);/g, (_, code) => String.fromCharCode(Number(code)))
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&");

/**
 * The Rust standard library, which every Rust program contains, and the crates it was
 * built from, from the copyright notices the Rust toolchain ships for it.
 */
function rustStandardLibrary() {
  const rustc = (...args) => execFileSync("rustc", args, { cwd: ROOT, encoding: "utf8" }).trim();
  const doc = join(rustc("--print", "sysroot"), "share/doc/rust");
  const page = join(doc, "COPYRIGHT-library.html");
  if (!existsSync(page)) fail(`the Rust toolchain has no ${page}`);
  const html = readFileSync(page, "utf8");
  const split = html.indexOf('<h2 id="out-of-tree-dependencies">');
  if (split < 0) fail(`${page} has no out-of-tree dependencies section`);
  const toolchainText = (spdx) => {
    const file = join(doc, "licenses", `${spdx}.txt`);
    return existsSync(file) ? addText(readFileSync(file, "utf8")) : null;
  };
  const inTree = decode(html.slice(html.indexOf("<body>"), split))
    .split("\n")
    .map((line) => line.trim())
    .filter((line, i, lines) => line || lines[i - 1])
    .join("\n");
  const std = {
    name: "Rust standard library",
    version: rustc("--version").split(" ")[1],
    licence: "Apache-2.0 OR MIT",
    source: "https://github.com/rust-lang/rust",
    texts: [addText(inTree), ...["MIT", "Apache-2.0", "Unicode-3.0"].map(toolchainText).filter(Boolean)],
  };
  const crates = html
    .slice(split)
    .split("<h3>📦 ")
    .slice(1)
    .map((section) => {
      const [, name, version] = section.match(/^(.+)-(\d[^<]*)<\/h3>/) ?? fail(`can't read ${page}`);
      const licence = decode(section.match(/<b>License:<\/b>([^<]*)/)?.[1] ?? "").trim();
      const source = section.match(/<b>URL:<\/b>\s*<a href="([^"]+)"/)?.[1] ?? `https://crates.io/crates/${name}`;
      let ids = [...section.matchAll(/<pre>([\s\S]*?)<\/pre>/g)].map(([, text]) => addText(decode(text)));
      if (!ids.length) ids = licence.split(/\s+(?:OR|AND|WITH)\s+|[()]/).map((s) => toolchainText(s.trim())).filter(Boolean);
      if (!ids.length) {
        const authors = decode(section.match(/<b>Authors:<\/b>([^<]*)/)?.[1] ?? "").split(",").map((a) => a.trim());
        ids = [standardText(licence, authors.filter(Boolean), name)].filter(Boolean);
      }
      if (!ids.length) fail(`${name} ${version} (Rust standard library) has no licence text`);
      return { name, version, licence, source, texts: ids };
    });
  return [std, ...crates.sort(byName)];
}

function jsPackages() {
  const lock = JSON.parse(readFileSync(join(DESKTOP, "package-lock.json"), "utf8"));
  return Object.entries(lock.packages)
    .filter(([path, entry]) => path.startsWith("node_modules/") && !entry.dev && !entry.devOptional)
    .map(([path, entry]) => {
      const dir = join(DESKTOP, path);
      if (!existsSync(dir)) fail(`${path} isn't installed: run npm ci`);
      const manifest = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
      const licence = manifest.license ?? entry.license;
      const allowed = licence?.split(/\s+OR\s+|[()]/).some((id) => JS_ALLOWED.has(id.trim()));
      if (!allowed) fail(`${manifest.name} is licensed "${licence}", which isn't allowed`);
      const ids = licenceFiles(dir);
      if (!ids.length) fail(`${manifest.name} ships no licence text`);
      const repo = typeof manifest.repository === "string" ? manifest.repository : manifest.repository?.url;
      return {
        name: manifest.name,
        version: manifest.version,
        licence,
        source: repo?.replace(/^git\+/, "").replace(/\.git$/, "") ?? `https://www.npmjs.com/package/${manifest.name}`,
        texts: ids,
      };
    });
}

function nativeLibraries() {
  const script = readFileSync(join(ROOT, "scripts/build-native-macos.sh"), "utf8");
  const value = (name) => script.match(new RegExp(`^${name}=(.+)$`, "m"))?.[1] ?? fail(`no ${name} in build-native-macos.sh`);
  const version = (name) => value(name).replace(/^[A-Za-z-]+-/, "");
  const licences = join(DESKTOP, "src-tauri/licenses");
  const files = (...names) => names.map((n) => addText(readFileSync(join(licences, n), "utf8")));
  return [
    {
      name: "LibRaw",
      version: version("LIBRAW"),
      licence: "LGPL-2.1 OR CDDL-1.0",
      source: value("LIBRAW_URL").replace("$LIBRAW", value("LIBRAW")),
      texts: files("LibRaw-COPYRIGHT", "LibRaw-LICENSE.LGPL", "LibRaw-LICENSE.CDDL"),
      note:
        "Used under the GNU Lesser General Public License 2.1, unmodified, as a separate library: " +
        "Contents/Frameworks/libraw_r.25.dylib in the app. You may replace it with your own build of " +
        `LibRaw ${version("LIBRAW").replace(/\.\d+$/, "")}, then sign the app again (for example, codesign --force ` +
        "--deep --sign - followed by the app's path). Its source is at the address below.",
    },
    {
      name: "libjpeg-turbo",
      version: version("TURBO"),
      licence: "IJG AND BSD-3-Clause AND Zlib",
      source: "https://libjpeg-turbo.org",
      texts: files("libjpeg-turbo-LICENSE.md"),
    },
    {
      name: "LLVM OpenMP",
      version: value("LLVM_VERSION"),
      licence: "Apache-2.0 WITH LLVM-exception",
      source: "https://github.com/llvm/llvm-project/tree/main/openmp",
      texts: files("LLVM-OpenMP-LICENSE.TXT"),
    },
  ];
}

const output = process.argv[2] ?? fail("usage: third-party-notices.mjs <output.json>");
const byName = (a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version);
const js = jsPackages().sort(byName);
const isFont = (p) => p.name.startsWith("@fontsource");
const groups = [
  { title: "Native libraries", entries: nativeLibraries() },
  { title: "Fonts", entries: js.filter(isFont) },
  { title: "JavaScript packages", entries: js.filter((p) => !isFont(p)) },
  { title: "Rust crates", entries: rustCrates().sort(byName) },
  { title: "Rust standard library", entries: rustStandardLibrary() },
];
const notices = { groups, texts: Object.fromEntries([...texts].sort(([a], [b]) => a.localeCompare(b))) };
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, JSON.stringify(notices));
const count = groups.reduce((n, g) => n + g.entries.length, 0);
console.log(`third-party-notices: ${count} libraries, ${texts.size} licence texts → ${output}`);
