import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { digest, buildManifest } from '../upstream-parity/import.mjs';
import { summarizeMatrix, verifiedFiles, profiles } from './run.mjs';
import { fileURLToPath } from 'node:url';

test('verify original corpus bytes before copying into version profiles', () => {
  const directory = mkdtempSync(path.join(os.tmpdir(), 'oxvelte-matrix-'));
  try {
    const source = Buffer.from('original fixture\r\n');
    writeFileSync(path.join(directory, 'input.svelte'), source);
    const manifest = { files: { 'input.svelte': digest(source) } };
    assert.deepEqual(verifiedFiles(directory, manifest).get('input.svelte'), source);
    writeFileSync(path.join(directory, 'input.svelte'), 'edited fixture');
    assert.throws(() => verifiedFiles(directory, manifest), /Corpus checksum differs/);
    assert.throws(() => verifiedFiles(directory, { files: { '../outside': '' } }), /Unsafe corpus path/);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('report union coverage without hiding regressions in another environment', () => {
  const original = { revision: 'pin', cases: [{ id: 'old', ineligible: ['version'] }, { id: 'new', ineligible: ['version'] }, { id: 'base', ineligible: [] }] };
  const report = (cases) => ({ environment: {}, totals: {}, cases: cases.map(([id, status]) => ({ id, status, issues: status === 'pass' ? {} : { reason: true } })) });
  const summary = summarizeMatrix(original, {
    modern: report([['old', 'version_skip'], ['new', 'pass']]),
    legacy: report([['old', 'pass'], ['new', 'gap']]),
  });
  assert.equal(summary.originallySkipped, 2);
  assert.equal(summary.covered, 2);
  assert.equal(summary.withGaps, 1);
  assert.deepEqual(summary.uncovered, []);
  assert.throws(() => summarizeMatrix(original, { missing: report([]) }), /report is missing old/);
});


test('pinned matrix declarations make every original skip eligible somewhere', () => {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const corpus = path.join(root, 'fixtures/upstream/eslint-plugin-svelte');
  const original = JSON.parse(readFileSync(path.join(corpus, 'manifest.json')));
  const files = verifiedFiles(corpus, original);
  const eligible = new Set();
  for (const profile of profiles) {
    const versions = JSON.parse(readFileSync(path.join(root, 'scripts/version-matrix', profile, 'package.json'))).dependencies;
    const manifest = buildManifest(files, versions);
    for (const entry of manifest.cases) if (!entry.ineligible.length) eligible.add(entry.id);
  }
  assert.deepEqual(original.cases.filter(entry => entry.ineligible.length && !eligible.has(entry.id)).map(entry => entry.id), []);
});
