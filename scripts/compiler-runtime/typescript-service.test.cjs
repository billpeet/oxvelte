const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createSession } = require('./typescript-service.cjs');

function session(script, extras = {}) {
  const source = `<script lang="ts">${script}</script>${extras.markup || ''}`;
  const start = source.indexOf('>') + 1;
  return createSession({ source, filename: path.join(os.tmpdir(), '__oxvelte-service-test__.svelte'),
    scripts: [{ start, end: start + script.length, lang: 'ts' }], ...extras });
}
function nodes(service, predicate) {
  const found = [];
  function visit(node) { if (predicate(node)) found.push(node); service.ts.forEachChild(node, visit); }
  visit(service.sourceFile);
  return found;
}
function conditions(service) { return nodes(service, node => service.ts.isIfStatement(node)).map(node => node.expression); }
function fixture(t, files) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'oxvelte-types-'));
  for (const [filename, content] of Object.entries(files)) fs.writeFileSync(path.join(directory, filename), content);
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  return directory;
}

test('genuine checker follows flow-sensitive assignments, unions, any and unknown', () => {
  const service = session('let x: string | null = null; if (x) {} x = "yes"; if (x) {}\ndeclare const a: any; if (a) {} declare const u: unknown; if (u) {}');
  assert.deepEqual(conditions(service).map(node => service.checker.typeToString(service.type(node))), ['null', 'string', 'any', 'unknown']);
});

test('project imports and compiler paths resolve against the component project', t => {
  const directory = fixture(t, {
    'tsconfig.json': JSON.stringify({ compilerOptions: { strict: true, baseUrl: '.', paths: { '@model': ['./model.ts'] } } }),
    'model.ts': 'export interface Model { present: true }',
  });
  const service = session('import type { Model } from "@model"; declare const model: Model; if (model.present) {}', {
    filename: path.join(directory, 'Component.svelte'),
  });
  assert.equal(service.checker.typeToString(service.type(conditions(service)[0])), 'true');
  assert.equal(service.program.getSemanticDiagnostics(service.sourceFile).length, 0);
});

test('component overlay supersedes an existing virtual file and respects strict project options', t => {
  const directory = fixture(t, {
    'tsconfig.json': JSON.stringify({ compilerOptions: { strict: false, strictNullChecks: false } }),
    'Component.svelte.ts': 'const value = 99;',
    'other.json': JSON.stringify({ compilerOptions: { strict: true, strictNullChecks: false } }),
  });
  const service = session('const value = null; if (value) {}', { filename: path.join(directory, 'Component.svelte') });
  assert.equal(service.compilerOptions.strictNullChecks, false);
  assert.equal(service.checker.typeToString(service.type(conditions(service)[0])), 'any');
  const explicit = session('if (true) {}', { filename: path.join(directory, 'Component.svelte'), settings: { typescript: { project: 'other.json' } } });
  assert.equal(explicit.compilerOptions.strict, true);
  assert.equal(explicit.compilerOptions.strictNullChecks, false);
});

test('reactive exclusions resolve binding identity, destructuring and shadowed locals', () => {
  const service = session('let root = null; let { a } = {a:null}; const fixed = null; if (root) {}\n$: { if (root) {} if (a) {} if (fixed) {} {let root = null; if (root) {}} }');
  assert.deepEqual(conditions(service).map(node => service.isReactive(node)), [false, true, true, false, false]);
});

test('UTF-16 positions and CRLF survive masking and parent pointers are available', () => {
  const script = '\r\nconst emoji = "😀";\r\nif (emoji) {}';
  const service = session(script, { markup: '<div>😀\r\ntext</div>' });
  const condition = conditions(service)[0];
  assert.equal(condition.parent.expression, condition);
  assert.equal(service.range(condition).start, service.source.lastIndexOf('emoji'));
  assert.equal(service.source.slice(service.range(condition).start, service.range(condition).end), 'emoji');
  assert.equal(service.sourceFile.text.length, service.source.length);
  assert.deepEqual(service.sourceFile.getLineAndCharacterOfPosition(condition.getStart()), {line:2,character:4});
});

test('template queries are checked, project to original text and suppress only mutable roots', () => {
  const script = 'let mutable = null; const fixed = null;';
  const markup = '{#if mutable}a{/if}{#if fixed}b{/if}{null ?? 1}';
  const source = `<script>${script}</script>${markup}`;
  const service = session(script, { source, templates: [
    {start:source.indexOf('mutable', source.indexOf('</script>')), end:source.indexOf('mutable', source.indexOf('</script>')) + 7,kind:'condition'},
    {start:source.lastIndexOf('fixed'),end:source.lastIndexOf('fixed') + 5,kind:'condition'},
    {start:source.indexOf('null ??'),end:source.indexOf('null ??') + 9,kind:'expression'},
  ], scripts:[{start:8,end:8+script.length,lang:'ts'}] });
  assert.deepEqual(conditions(service).map(node => service.isReactive(node)), [true, false]);
  assert.deepEqual(conditions(service).map(node => source.slice(service.range(node).start, service.range(node).end)), ['mutable', 'fixed']);
  const binary = nodes(service, node => service.ts.isBinaryExpression(node))[0];
  assert.equal(source.slice(service.range(binary).start, service.range(binary).end), 'null ?? 1');
  assert.equal(service.range(conditions(service)[0].parent), null);
});

test('invalid explicit project and source ranges give actionable errors', t => {
  const directory = fixture(t, {});
  assert.throws(() => session('if (true) {}', { filename:path.join(directory,'Component.svelte'),settings:{typescript:{project:'missing.json'}} }), /Unable to read TypeScript configuration/);
  assert.throws(() => session('if (true) {}', {scripts:[{start:-1,end:0}]}), /UTF-16 source offsets/);
});
