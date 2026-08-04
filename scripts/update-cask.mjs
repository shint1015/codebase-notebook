#!/usr/bin/env node
// Regenerate the Homebrew cask in ../homebrew-tap from a published release.
//
//   node scripts/update-cask.mjs [vX.Y.Z]   (defaults to the latest release)
//
// Reads the DMG sha256 digests from the GitHub API (via `gh`), rewrites
// Casks/codebase-notebook.rb in the sibling homebrew-tap checkout, and
// leaves committing/pushing to the caller.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const REPO = "shint1015/codebase-notebook";
const tapDir = join(dirname(dirname(fileURLToPath(import.meta.url))), "..", "homebrew-tap");
const caskPath = join(tapDir, "Casks", "codebase-notebook.rb");
if (!existsSync(caskPath)) {
  console.error(`homebrew-tap checkout not found at ${tapDir}`);
  process.exit(1);
}

const tagArg = process.argv[2];
const endpoint = tagArg
  ? `repos/${REPO}/releases/tags/${tagArg}`
  : `repos/${REPO}/releases/latest`;
const release = JSON.parse(execFileSync("gh", ["api", endpoint], { encoding: "utf8" }));
if (release.draft) {
  console.error(`${release.tag_name} is still a draft — publish it first.`);
  process.exit(1);
}
const version = release.tag_name.replace(/^v/, "");

const digest = (suffix) => {
  const name = `Codebase.Notebook_${version}_${suffix}.dmg`;
  const asset = release.assets.find((a) => a.name === name);
  if (!asset?.digest?.startsWith("sha256:")) {
    console.error(`missing sha256 digest for ${name}`);
    process.exit(1);
  }
  return asset.digest.slice("sha256:".length);
};
const arm = digest("aarch64");
const intel = digest("x64");

let cask = readFileSync(caskPath, "utf8");
cask = cask
  .replace(/version "[^"]+"/, `version "${version}"`)
  .replace(/arm: {3}"[0-9a-f]{64}"/, `arm:   "${arm}"`)
  .replace(/intel: "[0-9a-f]{64}"/, `intel: "${intel}"`);
writeFileSync(caskPath, cask);
console.log(`updated cask to ${version}\n  arm:   ${arm}\n  intel: ${intel}`);
console.log(`now: cd ${tapDir} && git commit -am "Update cask to ${version}" && git push`);
