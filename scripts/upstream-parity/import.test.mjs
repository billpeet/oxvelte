import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildManifest, companion, configCandidates, ineligible, isInput, ruleMetadata } from './import.mjs';

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
  files.set('src/rules/r.ts', Buffer.from("export default createRule('r', { meta: { fixable: 'code' } });"));
  assert.throws(() => buildManifest(files), /Missing expected fix output/);
  files.set('tests/fixtures/rules/r/invalid/test-output.svelte', Buffer.from('<div />'));
  assert.equal(buildManifest(files).cases[0].output, null);
});

test('inherited metadata comes from the declared ESLint core rule and respects overrides', () => {
  const source = `import { getCoreRule as load } from '../utils/eslint-core.js';
  const parent = load('prefer-const');
  export default createRule('wrapper', { meta: { ...parent.meta }, create(context) {} });`;
  assert.equal(ruleMetadata(source).fixable, true);
  assert.equal(ruleMetadata(source.replace('...parent.meta', '...parent.meta, fixable: null')).fixable, false);
  assert.equal(ruleMetadata(source.replace('...parent.meta', "...parent.meta, 'fixable': null")).fixable, false);
  assert.equal(ruleMetadata("export default createRule('r', { meta: { 'fixable': 'code' } });").fixable, true);
  assert.equal(ruleMetadata(source.replace('prefer-const', 'no-inner-declarations')).fixable, false);
  assert.throws(() => ruleMetadata(source.replace('prefer-const', 'unknown-core-rule')), /Unknown inherited ESLint core rule unknown-core-rule/);
  assert.equal(ruleMetadata(`// fixable: 'code'\nexport default createRule('r', { meta: {} });`).fixable, false);
});

test('only a proven module compiler-version gate returning no listeners makes cases ineligible', () => {
  const source = `import { VERSION as version } from 'svelte/compiler'; import semver from 'semver';
  const enabled = semver.satisfies(version, '>=5.56.0');
  export default createRule('r', { meta: {}, create(context) { if (!enabled) { return {}; } return { listener() {} }; } });`;
  assert.deepEqual(ruleMetadata(source).ineligible, ['Rule runtime gate: svelte 5.49.2 does not satisfy >=5.56.0']);
  assert.deepEqual(ruleMetadata(source, {svelte: '5.56.0'}).ineligible, []);
  assert.deepEqual(ruleMetadata(source.replace('return {};', 'return { listener() {} };')).ineligible, []);
  assert.deepEqual(ruleMetadata(source.replace("'svelte/compiler'", "'other'")).ineligible, []);
  assert.deepEqual(ruleMetadata(source.replace('const enabled', 'let enabled')).ineligible, []);
  assert.deepEqual(ruleMetadata(source.replace('create(context)', 'create(enabled)')).ineligible, []);
});


test('version variants recompute eligibility and inherited metadata without changing expectations', () => {
  const source = `import { VERSION } from 'svelte/compiler'; import semver from 'semver';
    import { getCoreRule } from '../utils/eslint-core.js'; const core = getCoreRule('example');
    const enabled = semver.satisfies(VERSION, '>=5.56.0');
    export default createRule('r', {meta:{...core.meta},create(context){if(!enabled){return {};}return {listener(){}};}});`;
  const files = new Map([
    ['src/rules/r.ts', Buffer.from(source)],
    ['tests/fixtures/rules/r/invalid/test-input.svelte', Buffer.from('<div/>')],
    ['tests/fixtures/rules/r/invalid/test-output.svelte', Buffer.from('<p/>')],
    ['tests/fixtures/rules/r/invalid/test-errors.yaml', Buffer.from('- message: original\n  line: 1\n  column: 1\n')],
    ['tests/fixtures/rules/r/invalid/test-requirements.json', Buffer.from('{"svelte":">=5.56.0"}')],
  ]);
  const rules = new Map([['example', {meta:{fixable:'code'}}]]);
  const old = buildManifest(files, {svelte:'5.49.2'}, rules);
  const modern = buildManifest(files, {svelte:'5.56.0'}, rules);
  assert.equal(old.cases[0].ineligible.length, 2);
  assert.deepEqual(modern.cases[0].ineligible, []);
  assert.deepEqual(modern.cases[0].errors, old.cases[0].errors);
  assert.equal(modern.cases[0].output, '<p/>');
  assert.deepEqual(modern.files, old.files);
  assert.deepEqual(modern.environment, {svelte:'5.56.0'});
});
