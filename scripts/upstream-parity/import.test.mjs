import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildManifest, companion, configCandidates, ineligible, isInput } from './import.mjs';

test('upstream discovery includes route and rune modules, excluding helper files', () => {
  for (const name of ['test-input.svelte', 'test-input.svelte.js', 'test-input.svelte.ts', '+page.svelte', '+page.server.ts']) assert.ok(isInput(name));
  for (const name of ['_input.svelte', 'shared-types.ts', 'test-output.svelte', 'test-errors.yaml']) assert.ok(!isInput(name));
  assert.equal(companion('rule/invalid/+page.server.ts', 'errors.yaml'), 'rule/invalid/errors.yaml');
  assert.equal(companion('rule/invalid/test-input.svelte.ts', 'config.json'), 'rule/invalid/test-config.json');
});

test('config precedence and dependency gates match the upstream loader', () => {
  assert.deepEqual(configCandidates('r/test-input.js'), ['r/test-config.json', 'r/_config.json', 'r/test-config.js', 'r/_config.js', 'r/test-config.cjs', 'r/_config.cjs']);
  assert.deepEqual(ineligible({ svelte: '^4 || ^3', FIXME: 'comment' }), ['svelte 5.49.2 does not satisfy ^4 || ^3']);
  assert.deepEqual(ineligible({ svelte: '>=5.0.0-0', typescript: '>=5.3.0' }), []);
  assert.throws(() => ineligible({ unknown: '*' }), /No declared version/);
});

test('import fails on absent expectations and preserves exact YAML diagnostics', () => {
  const files = new Map([['tests/fixtures/rules/r/invalid/test-input.svelte', Buffer.from('<div />')]]);
  assert.throws(() => buildManifest(files), /Missing expected diagnostics/);
  files.set('tests/fixtures/rules/r/invalid/test-errors.yaml', Buffer.from('- message: bad\n  line: 1\n  column: 2\n  suggestions: null\n'));
  assert.deepEqual(buildManifest(files).cases[0].errors, [{ message: 'bad', line: 1, column: 2, suggestions: null }]);
  files.set('src/rules/r.ts', Buffer.from("fixable: 'code'"));
  assert.throws(() => buildManifest(files), /Missing expected fix output/);
  files.set('tests/fixtures/rules/r/invalid/test-output.svelte', Buffer.from('<div />'));
  assert.equal(buildManifest(files).cases[0].output, null);
});
