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
 *   - CHANGELOG.md ([Unreleased] is promoted — stable releases only)
 *
 * Branching model:
 *   - Stable versions (x.y.z) are cut on `main` when develop is merged:
 *     patch / minor / major. From a -dev prerelease they FINALIZE the
 *     version (0.3.0-dev.4 --minor-> 0.3.0), matching node-semver.
 *   - `dev` marks work on the `develop` branch as a prerelease:
 *     0.2.0 -dev-> 0.3.0-dev.0 -dev-> 0.3.0-dev.1 ...
 *
 * Usage:
 *   node scripts/bump-version.mjs patch|minor|major|dev
 *   node scripts/bump-version.mjs 1.2.3 | 1.3.0-dev.2
 */
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");
const write = (p, content) => writeFileSync(join(root, p), content);

const SEMVER = /^(\d+)\.(\d+)\.(\d+)(?:-dev\.(\d+))?$/;

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function parse(version) {
  const m = SEMVER.exec(version);
  if (!m) fail(`unexpected version: ${version}`);
  return {
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    dev: m[4] === undefined ? null : Number(m[4]),
  };
}

function bump(current, kind) {
  const v = parse(current);
  const isPre = v.dev !== null;
  switch (kind) {
    // Stable bumps follow node-semver: bumping a prerelease finalizes it
    // when the corresponding parts are already zeroed.
    case "patch":
      return isPre
        ? `${v.major}.${v.minor}.${v.patch}`
        : `${v.major}.${v.minor}.${v.patch + 1}`;
    case "minor":
      return isPre && v.patch === 0
        ? `${v.major}.${v.minor}.0`
        : `${v.major}.${v.minor + 1}.0`;
    case "major":
      return isPre && v.minor === 0 && v.patch === 0
        ? `${v.major}.0.0`
        : `${v.major + 1}.0.0`;
    case "dev":
      return isPre
        ? `${v.major}.${v.minor}.${v.patch}-dev.${v.dev + 1}`
        : `${v.major}.${v.minor + 1}.0-dev.0`;
    default:
      return fail(`unknown bump type: ${kind}`);
  }
}

const arg = process.argv[2];
if (!arg) fail("usage: bump-version.mjs <patch|minor|major|dev|x.y.z[-dev.N]>");

const pkg = JSON.parse(read("package.json"));
const current = pkg.version;
parse(current); // validate

const next = SEMVER.test(arg) ? arg : bump(current, arg);
if (next === current) fail(`already at ${current}`);
const isStableRelease = parse(next).dev === null;

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
  /^version = "[^"]+"$/m,
  `version = "${next}"`,
);
if (bumpedToml === cargoToml) fail("could not find version in Cargo.toml");
write("src-tauri/Cargo.toml", bumpedToml);

// src-tauri/Cargo.lock — only the codebase-notebook package block.
const cargoLock = read("src-tauri/Cargo.lock");
const bumpedLock = cargoLock.replace(
  /(name = "codebase-notebook"\nversion = ")[^"]+(")/,
  `$1${next}$2`,
);
if (bumpedLock === cargoLock) fail("could not find codebase-notebook in Cargo.lock");
write("src-tauri/Cargo.lock", bumpedLock);

// CHANGELOG.md — promote [Unreleased] for stable releases only.
if (isStableRelease) {
  const today = new Date().toISOString().slice(0, 10);
  const changelog = read("CHANGELOG.md");
  if (!changelog.includes("## [Unreleased]")) fail("CHANGELOG.md has no [Unreleased] section");
  const promoted = changelog.replace(
    "## [Unreleased]",
    `## [Unreleased]\n\n## [${next}] - ${today}`,
  );
  write("CHANGELOG.md", promoted);
}

console.log(`version: ${current} -> ${next}`);
console.log("");
console.log(
  `updated: package.json, package-lock.json, tauri.conf.json, Cargo.toml, Cargo.lock${isStableRelease ? ", CHANGELOG.md" : ""}`,
);
console.log("");
if (isStableRelease) {
  console.log("next steps (release on main):");
  console.log(`  1. review CHANGELOG.md (move notes out of [Unreleased] if needed)`);
  console.log(`  2. git add -A && git commit -m "Release v${next}"`);
  console.log(`  3. git tag v${next}`);
  console.log(`  4. git checkout develop && git merge main`);
} else {
  console.log("next steps (develop):");
  console.log(`  git add -A && git commit -m "Start ${next}"`);
}
