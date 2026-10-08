#!/usr/bin/env node
"use strict";

// Launcher for the native oxvelte binary. The binary itself ships in a
// per-platform package that npm installs through optionalDependencies;
// scripts/stage-npm.mjs generates those packages and must agree with the
// naming scheme used here.

const { spawnSync } = require("node:child_process");

function isMusl() {
  // glibc builds of Node report their glibc version; musl builds do not.
  const report = process.report && process.report.getReport();
  return !(report && report.header && report.header.glibcVersionRuntime);
}

function platformPackage() {
  const { platform, arch } = process;
  let abi = "";
  if (platform === "linux") abi = isMusl() ? "-musl" : "-gnu";
  if (platform === "win32") abi = "-msvc";
  return `@billpeet/oxvelte-${platform}-${arch}${abi}`;
}

const pkg = platformPackage();
const exe = process.platform === "win32" ? "oxvelte.exe" : "oxvelte";

let binary;
try {
  binary = require.resolve(`${pkg}/${exe}`);
} catch {
  console.error(
    `oxvelte: could not find the native binary package "${pkg}".\n` +
      `If ${process.platform}-${process.arch} is a supported platform, reinstall with optional ` +
      `dependencies enabled (no --omit=optional / --no-optional).\n` +
      `Otherwise build from source: cargo install --git https://github.com/billpeet/oxvelte.git`,
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  console.error(`oxvelte: failed to run ${binary}: ${result.error.message}`);
  process.exit(1);
}
if (result.signal) {
  process.kill(process.pid, result.signal);
}
process.exit(result.status === null ? 1 : result.status);
