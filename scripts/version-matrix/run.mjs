import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildManifest, digest } from '../upstream-parity/import.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const baseCorpus = path.join(root, 'fixtures/upstream/eslint-plugin-svelte');
const profilesRoot = path.join(root, 'scripts/version-matrix');
const outputRoot = path.join(root, 'reports/version-matrix');
export const profiles = ['modern', 'svelte4', 'legacyparser', 'svelte3'];
const versionNames = ['svelte', 'eslint', 'typescript', '@typescript-eslint/parser', 'svelte-eslint-parser'];

export function verifiedFiles(corpus, manifest) {
  const files = new Map();
  for (const [name, hash] of Object.entries(manifest.files)) {
    if (path.isAbsolute(name) || name.split('/').includes('..') || name.includes('\\')) throw new Error(`Unsafe corpus path: ${name}`);
    const bytes = readFileSync(path.join(corpus, ...name.split('/')));
    if (digest(bytes) !== hash) throw new Error(`Corpus checksum differs: ${name}`);
    files.set(name, bytes);
  }
  return files;
}

export function prepareProfile(name) {
  if (!profiles.includes(name)) throw new Error(`Unknown profile: ${name}`);
  const runtime = path.join(profilesRoot, name);
  const declared = JSON.parse(readFileSync(path.join(runtime, 'package.json'))).dependencies;
  for (const file of ['babel.config.cjs', 'postcss.config.cjs']) {
    writeFileSync(path.join(runtime, file), readFileSync(path.join(root, 'scripts/compiler-runtime', file)));
  }
  const require = createRequire(path.join(runtime, 'package.json'));
  const versions = Object.fromEntries(versionNames.map((dependency) => {
    const installed = require(`${dependency}/package.json`).version;
    if (installed !== declared[dependency]) throw new Error(`${name}: installed ${dependency} ${installed} differs from pin ${declared[dependency]}`);
    return [dependency, installed];
  }));
  const original = JSON.parse(readFileSync(path.join(baseCorpus, 'manifest.json')));
  const files = verifiedFiles(baseCorpus, original);
  // This generated file signals the selected compiler major to native rules.
  // Imported fixture sources, expected diagnostics and fixes remain byte-exact.
  files.set('tests/package.json', Buffer.from(JSON.stringify({ private: true, dependencies: { svelte: versions.svelte } }, null, 2) + '\n'));
  const { builtinRules } = require('eslint/use-at-your-own-risk');
  const manifest = buildManifest(files, versions, builtinRules);
  const corpus = path.join(outputRoot, name, 'corpus');
  for (const [file, bytes] of files) {
    const destination = path.join(corpus, ...file.split('/'));
    mkdirSync(path.dirname(destination), { recursive: true });
    writeFileSync(destination, bytes);
  }
  writeFileSync(path.join(corpus, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  return { name, runtime, corpus, environment: versions, report: path.join(outputRoot, name, 'parity.json') };
}

export function summarizeMatrix(original, reports) {
  const originalSkips = original.cases.filter((entry) => entry.ineligible.length).map((entry) => entry.id);
  const skippedCases = originalSkips.map((id) => {
    const outcomes = Object.fromEntries(Object.entries(reports).map(([name, report]) => {
      const entry = report.cases.find((candidate) => candidate.id === id);
      if (!entry) throw new Error(`${name} report is missing ${id}`);
      return [name, { status: entry.status, issues: entry.issues }];
    }));
    const values = Object.values(outcomes);
    return { id, covered: values.some((entry) => entry.status === 'pass'),
      gaps: values.some((entry) => entry.status === 'gap'), outcomes };
  });
  return { revision: original.revision, profiles: Object.fromEntries(Object.entries(reports).map(([name, report]) => [name, { environment: report.environment, totals: report.totals }])),
    originallySkipped: originalSkips.length, covered: skippedCases.filter((entry) => entry.covered).length,
    withGaps: skippedCases.filter((entry) => entry.gaps).length,
    uncovered: skippedCases.filter((entry) => !entry.covered).map((entry) => entry.id), cases: skippedCases };
}

function install(name) {
  // npm's Windows launcher needs a shell. The profile is validated above;
  // quoting the absolute prefix also supports workspace paths with spaces.
  const args = ['ci', '--prefix', path.join(profilesRoot, name), '--ignore-scripts', '--no-audit', '--no-fund'];
  const result = process.platform === 'win32'
    ? spawnSync('npm.cmd', args.map((arg) => `"${arg}"`), { cwd: root, stdio: 'inherit', shell: true })
    : spawnSync('npm', args, { cwd: root, stdio: 'inherit' });
  if (result.error || result.status !== 0) throw new Error(`npm ci failed for ${name}: ${result.error?.message ?? result.status}`);
}

function main() {
  const args = process.argv.slice(2);
  let selected = profiles;
  let prepareOnly = false;
  let noInstall = false;
  for (let index = 0; index < args.length; index++) {
    if (args[index] === '--profile') selected = [args[++index]];
    else if (args[index] === '--prepare-only') prepareOnly = true;
    else if (args[index] === '--no-install') noInstall = true;
    else throw new Error('Usage: node scripts/version-matrix/run.mjs [--profile NAME] [--prepare-only] [--no-install]');
  }
  for (const name of selected) if (!profiles.includes(name)) throw new Error(`Unknown profile: ${name}`);
  const reports = {};
  let failed = false;
  for (const name of selected) {
    if (!noInstall) install(name);
    const profile = prepareProfile(name);
    console.log(`${name}: prepared unchanged fixtures for ${JSON.stringify(profile.environment)}`);
    if (prepareOnly) continue;
    rmSync(profile.report, { force: true });
    const result = spawnSync('cargo', ['test', '--locked', '--test', 'upstream_parity', '--', '--corpus', profile.corpus,
      '--strict', '--no-baseline', '--report', profile.report], {
      cwd: root, stdio: 'inherit', env: { ...process.env, OXVELTE_COMPILER_RUNTIME: profile.runtime },
    });
    if (result.error) throw result.error;
    failed ||= result.status !== 0;
    if (!existsSync(profile.report)) throw new Error(`No parity report generated for ${name}`);
    const report = JSON.parse(readFileSync(profile.report));
    if (JSON.stringify(report.environment) !== JSON.stringify(profile.environment)) throw new Error(`Report environment differs for ${name}`);
    if (report.manifestHash !== digest(readFileSync(path.join(profile.corpus, 'manifest.json')))) throw new Error(`Report corpus differs for ${name}`);
    reports[name] = report;
  }
  if (!prepareOnly) {
    const original = JSON.parse(readFileSync(path.join(baseCorpus, 'manifest.json')));
    const summary = summarizeMatrix(original, reports);
    mkdirSync(outputRoot, { recursive: true });
    writeFileSync(path.join(outputRoot, selected.length === profiles.length ? 'summary.json' : `summary-${selected[0]}.json`), JSON.stringify(summary, null, 2) + '\n');
    console.log(`Original ${summary.originallySkipped} skips: ${summary.covered} covered, ${summary.withGaps} with gaps, ${summary.uncovered.length} without a pass.`);
  }
  if (failed) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
