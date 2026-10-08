import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const repo = dirname(dirname(fileURLToPath(import.meta.url)));
const runtime = join(repo, 'scripts/compiler-runtime');
const fixtures = join(repo, 'fixtures/upstream/eslint-plugin-svelte/tests/fixtures/rules/valid-compile');
function request(input) {
  const child = spawnSync(process.execPath, [join(runtime, 'bridge.cjs')], {
    input: JSON.stringify(input) + '\n', encoding: 'utf8',
    env: { ...process.env, OXVELTE_COMPILER_RUNTIME: runtime },
  });
  assert.equal(child.status, 0, child.stderr);
  return JSON.parse(child.stdout.trim());
}

function compile(source, settings) { return request({source,settings}); }
function callbacks(source, settings, warnings) { return request({operation:'callbacks',source,settings,warnings}); }
test('executes all six eligible upstream compiler configuration modules', () => {
  for (const [kind, directory, name] of [
    ['invalid', 'svelte-config-custom-warn', 'a11y'],
    ['invalid', 'svelte-config-onwarn', 'a11y'],
    ['invalid', 'svelte-config-warning-filter', 'a11y'],
    ['valid', 'svelte-config-onwarn', 'a11y'],
    ['valid', 'svelte-config-warning-filter', 'a11y'],
    ['valid', 'svelte-config-experimental-async', 'top-level-await'],
  ]) {
    const path = join(fixtures, kind, directory);
    const source = readFileSync(join(path, name + '-input.svelte'), 'utf8');
    const settings = { compiler: { executableConfigPath: join(path, '_config.cjs') } };
    const response = compile(source, settings);
    assert.equal(response.error, undefined, directory);
    assert.equal(response.result.kind, 'warn', directory);
    const transformed = callbacks(source, settings, response.result.warnings);
    assert.equal(transformed.error, undefined, directory);
    const reported = transformed.result.warnings.filter(warning => !warning.filtered);
    if (kind === 'valid') assert.deepEqual(reported, [], directory);
    else if (directory === 'svelte-config-custom-warn') {
      assert.equal(reported.length, 2);
      assert.ok(reported.every(warning => warning.report.code === 'foo'));
    } else {
      assert.deepEqual(reported.map(warning => warning.code), ['a11y_autofocus']);
    }
  }
});

test('configured Babel syntax is parsed and transformed rather than discarded', () => {
  const body = '\nfunction func() {}\nconst obj = {};\nconst foo = obj::func;\n';
  const source = '<script>' + body + '</script>\n<img src="x">';
  const settings = {
    compiler: { parser: '@babel/eslint-parser' },
    _oxvelteScripts: [{ start: 8, end: 8 + Buffer.byteLength(body) }],
  };
  const valid = compile(source, settings);
  assert.equal(valid.error, undefined);
  assert.equal(valid.result.kind, 'warn');
  const warning = valid.result.warnings.find(warning => warning.code === 'a11y_missing_attribute');
  assert.equal(warning.start, source.indexOf('<img'));
  const invalid = compile(source.replace('obj::func', 'obj::'), settings);
  assert.match(invalid.error, /Unexpected token/);
});
