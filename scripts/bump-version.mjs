#!/usr/bin/env node
/**
 * Single-source version bump for Codebase Notebook.
 *
 * The canonical version lives in package.json; this script bumps it and
 * propagates the same version to every other manifest so they can never
 * drift:
 *   - package.json / package-lock.json
 *   - src-tauri/tauri.conf.json
 *   - src-tauri/Cargo.toml / Cargo.lock
 *   - CHANGELOG.md ([Unreleased] section is promoted to the new version)
 *
 * Usage:
 *   node scripts/bump-version.mjs patch|minor|major
 *   node scripts/bump-version.mjs 1.2.3
 */
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");
const write = (p, content) => writeFileSync(join(root, p), content);

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

const arg = process.argv[2];
if (!arg) fail("usage: bump-version.mjs <patch|minor|major|x.y.z>");

const pkg = JSON.parse(read("package.json"));
const current = pkg.version;
if (!/^\d+\.\d+\.\d+$/.test(current)) fail(`unexpected current version: ${current}`);

let next;
if (/^\d+\.\d+\.\d+$/.test(arg)) {
  next = arg;
} else {
  const [major, minor, patch] = current.split(".").map(Number);
  if (arg === "major") next = `${major + 1}.0.0`;
  else if (arg === "minor") next = `${major}.${minor + 1}.0`;
  else if (arg === "patch") next = `${major}.${minor}.${patch + 1}`;
  else fail(`unknown bump type: ${arg}`);
}
if (next === current) fail(`already at ${current}`);

// package.json
pkg.version = next;
write("package.json", JSON.stringify(pkg, null, 2) + "\n");

// package-lock.json
const lock = JSON.parse(read("package-lock.json"));
lock.version = next;
if (lock.packages && lock.packages[""]) lock.packages[""].version = next;
write("package-lock.json", JSON.stringify(lock, null, 2) + "\n");

// src-tauri/tauri.conf.json
const tauriConf = JSON.parse(read("src-tauri/tauri.conf.json"));
tauriConf.version = next;
write("src-tauri/tauri.conf.json", JSON.stringify(tauriConf, null, 2) + "\n");

// src-tauri/Cargo.toml — only the [package] version line (first match).
const cargoToml = read("src-tauri/Cargo.toml");
const bumpedToml = cargoToml.replace(
  /^version = "\d+\.\d+\.\d+"$/m,
  `version = "${next}"`,
);
if (bumpedToml === cargoToml) fail("could not find version in Cargo.toml");
write("src-tauri/Cargo.toml", bumpedToml);

// src-tauri/Cargo.lock — only the codebase-notebook package block.
const cargoLock = read("src-tauri/Cargo.lock");
const bumpedLock = cargoLock.replace(
  /(name = "codebase-notebook"\nversion = ")\d+\.\d+\.\d+(")/,
  `$1${next}$2`,
);
if (bumpedLock === cargoLock) fail("could not find codebase-notebook in Cargo.lock");
write("src-tauri/Cargo.lock", bumpedLock);

// CHANGELOG.md — promote [Unreleased] to the new version.
const today = new Date().toISOString().slice(0, 10);
const changelog = read("CHANGELOG.md");
if (!changelog.includes("## [Unreleased]")) fail("CHANGELOG.md has no [Unreleased] section");
const promoted = changelog.replace(
  "## [Unreleased]",
  `## [Unreleased]\n\n## [${next}] - ${today}`,
);
write("CHANGELOG.md", promoted);

console.log(`version: ${current} -> ${next}`);
console.log("");
console.log("updated: package.json, package-lock.json, tauri.conf.json, Cargo.toml, Cargo.lock, CHANGELOG.md");
console.log("");
console.log("next steps:");
console.log(`  1. review CHANGELOG.md (move notes out of [Unreleased] if needed)`);
console.log(`  2. git add -A && git commit -m "Release v${next}"`);
console.log(`  3. git tag v${next}`);
