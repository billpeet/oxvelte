#!/usr/bin/env node
// Stage the npm packages for a release into dist-npm/.
//
//   node scripts/stage-npm.mjs --binaries <dir> [--out dist-npm] [--allow-missing]
//
// <dir> must contain one folder per Rust target holding the built binary,
// e.g. <dir>/x86_64-unknown-linux-gnu/oxvelte. The version comes from
// Cargo.toml, so package.json files never need a manual bump.
//
// Output:
//   dist-npm/oxvelte             the main package (@billpeet/oxvelte)
//   dist-npm/oxvelte-<platform>  one binary package per target
//
// --allow-missing skips targets with no binary (for local testing); a real
// release must build them all.

import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const SCOPE = "@billpeet";

// Package names are `${SCOPE}/oxvelte-${os}-${cpu}[-${abi}]`, which is what
// npm/oxvelte/bin/oxvelte.js looks up at runtime. Keep the two in sync, along
// with the build matrix in .github/workflows/release.yml.
const TARGETS = [
  { target: "aarch64-apple-darwin", os: "darwin", cpu: "arm64" },
  { target: "x86_64-apple-darwin", os: "darwin", cpu: "x64" },
  { target: "x86_64-unknown-linux-gnu", os: "linux", cpu: "x64", abi: "gnu", libc: "glibc" },
  { target: "aarch64-unknown-linux-gnu", os: "linux", cpu: "arm64", abi: "gnu", libc: "glibc" },
  { target: "x86_64-unknown-linux-musl", os: "linux", cpu: "x64", abi: "musl", libc: "musl" },
  { target: "aarch64-unknown-linux-musl", os: "linux", cpu: "arm64", abi: "musl", libc: "musl" },
  { target: "x86_64-pc-windows-msvc", os: "win32", cpu: "x64", abi: "msvc" },
  { target: "aarch64-pc-windows-msvc", os: "win32", cpu: "arm64", abi: "msvc" },
];

const { values: args } = parseArgs({
  options: {
    binaries: { type: "string" },
    out: { type: "string", default: "dist-npm" },
    "allow-missing": { type: "boolean", default: false },
  },
});
if (!args.binaries) {
  console.error("usage: stage-npm.mjs --binaries <dir> [--out dist-npm] [--allow-missing]");
  process.exit(2);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binaries = resolve(args.binaries);
const out = resolve(args.out);

const cargoToml = readFileSync(join(root, "Cargo.toml"), "utf8");
const version = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
if (!version) throw new Error("could not read the version from Cargo.toml");

const main = JSON.parse(readFileSync(join(root, "npm/oxvelte/package.json"), "utf8"));

rmSync(out, { recursive: true, force: true });

const optionalDependencies = {};
const missing = [];

for (const t of TARGETS) {
  const exe = t.os === "win32" ? "oxvelte.exe" : "oxvelte";
  const binary = join(binaries, t.target, exe);
  if (!existsSync(binary)) {
    missing.push(t.target);
    continue;
  }

  const platform = [t.os, t.cpu, t.abi].filter(Boolean).join("-");
  const name = `${SCOPE}/oxvelte-${platform}`;
  const dir = join(out, `oxvelte-${platform}`);
  mkdirSync(dir, { recursive: true });

  copyFileSync(binary, join(dir, exe));
  chmodSync(join(dir, exe), 0o755);
  copyFileSync(join(root, "LICENSE"), join(dir, "LICENSE"));
  copyFileSync(join(root, "THIRD_PARTY_NOTICES"), join(dir, "THIRD_PARTY_NOTICES"));
  writeFileSync(
    join(dir, "README.md"),
    `# ${name}\n\nThe ${t.target} binary for [${main.name}](https://www.npmjs.com/package/${main.name}). ` +
      `It is installed automatically as an optional dependency of that package; install that instead.\n`,
  );
  writeFileSync(
    join(dir, "package.json"),
    JSON.stringify(
      {
        name,
        version,
        description: `The ${t.target} binary for ${main.name}.`,
        license: main.license,
        repository: main.repository,
        homepage: main.homepage,
        os: [t.os],
        cpu: [t.cpu],
        ...(t.libc && { libc: [t.libc] }),
        files: [exe, "THIRD_PARTY_NOTICES"],
        preferUnplugged: true,
        publishConfig: main.publishConfig,
      },
      null,
      2,
    ) + "\n",
  );

  optionalDependencies[name] = version;
}

if (missing.length && !args["allow-missing"]) {
  console.error(`missing binaries for: ${missing.join(", ")}\n(looked in ${binaries}/<target>/)`);
  process.exit(1);
}
if (!Object.keys(optionalDependencies).length) {
  console.error(`no binaries found in ${binaries}`);
  process.exit(1);
}

const mainDir = join(out, "oxvelte");
cpSync(join(root, "npm/oxvelte"), mainDir, { recursive: true });
copyFileSync(join(root, "README.md"), join(mainDir, "README.md"));
copyFileSync(join(root, "LICENSE"), join(mainDir, "LICENSE"));
writeFileSync(
  join(mainDir, "package.json"),
  JSON.stringify({ ...main, version, optionalDependencies }, null, 2) + "\n",
);

console.log(`Staged ${main.name}@${version} with ${Object.keys(optionalDependencies).length} platform package(s) in ${out}`);
if (missing.length) console.log(`Skipped (no binary): ${missing.join(", ")}`);
