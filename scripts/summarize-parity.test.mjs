import assert from 'node:assert/strict';
import { test } from 'node:test';
import { classify, summarize } from './summarize-parity.mjs';

const diag = (message, column = 1) => ({ message, line: 1, column });

test('diagnostic categories preserve duplicates and distinguish locations from messages', () => {
  assert.equal(classify({ diagnostics: { expected: [diag('foo'), diag('foo')], actual: [diag('foo')] } }), 'diagnostic-count');
  assert.equal(classify({ diagnostics: { expected: [diag('foo'), diag('foo')], actual: [diag('foo'), diag('bar')] } }), 'diagnostic-message');
  assert.equal(classify({ diagnostics: { expected: [diag('foo'), diag('bar')], actual: [diag('bar', 2), diag('foo', 3)] } }), 'diagnostic-location');
});

test('primary blockers do not discard overlapping fixes or suggestions', () => {
  const issues = { diagnostics: { expected: [diag('foo')], actual: [diag('foo', 2)] }, fix_output: { expected: 'a', actual: 'b' }, suggestion_capability: ['suggestion'] };
  const summary = summarize({ cases: [{ id: 'rule/invalid/input.svelte', rule: 'rule', status: 'gap', issues }], totals: { pass: 0, gaps: 1, versionSkip: 0 } });
  assert.equal(summary.categories[0].id, 'diagnostic-location');
  assert.equal(summary.categories[0].count, 1);
  assert.equal(summary.dimensions.fix_output.count, 1);
  assert.equal(summary.dimensions.suggestion_capability.count, 1);
  assert.equal(classify({ ...issues, parse_errors: ['invalid slot'] }), 'parser');
  assert.equal(classify({ ...issues, compiler_capability: 'compiler' }), 'compiler');
});

test('version skips stay separate, and missing/duplicated inventory fails', () => {
  const entry = { id: 'rule/valid/input.svelte', rule: 'rule', status: 'version_skip', issues: { version_skip: ['Svelte 4 required'] } };
  const report = { cases: [entry], totals: { pass: 0, gaps: 0, versionSkip: 1 } };
  assert.equal(summarize(report).categories.length, 0);
  assert.equal(summarize(report).versionSkips.rule.length, 1);
  assert.throws(() => summarize({ ...report, cases: [entry, entry] }), /Duplicate case/);
  assert.throws(() => summarize({ ...report, totals: { pass: 0, gaps: 1, versionSkip: 0 } }), /totals/);
  assert.throws(() => classify({ new_unknown_dimension: true }), /Unclassified/);
});

test('suggestion comparisons and invalid suggestion spans retain their categories', () => {
  const issues = { suggestions: { expected: [[{ desc: 'replace', output: 'a' }]], actual: [[]] } };
  assert.equal(classify(issues), 'suggestions');
  assert.equal(classify({ ...issues, suggestion_spans: 'Invalid fix span' }), 'invalid-spans');
  const summary = summarize({ cases: [{ id: 'rule/invalid/suggestion', rule: 'rule', status: 'gap', issues }], totals: { pass: 0, gaps: 1, versionSkip: 0 } });
  assert.equal(summary.dimensions.suggestions.count, 1);
});
