# eslint-plugin-svelte test suite parity

Researched on 2026-10-08. Yes, the upstream fixture suite is worth using as the main rule compatibility target. Most of its inputs are already in this repository. The immediate opportunity is to run their complete expectations and refresh the missing cases.

## Baseline and sources

The official repository is [`sveltejs/eslint-plugin-svelte`](https://github.com/sveltejs/eslint-plugin-svelte), rather than an `eslint` organization repository. This research pins main to [`18339c886320151148568063c5801bf69cb51027`](https://github.com/sveltejs/eslint-plugin-svelte/commit/18339c886320151148568063c5801bf69cb51027), dated 2026-10-03. The package at that revision declares version **3.23.0**. Pin the commit as well as the npm version because unpublished fixture changes can follow a release. [Package source](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/package.json).

The current JobSys lock independently records plugin 3.23.0, ESLint 10.9.1, Svelte 5.49.2, svelte-eslint-parser 1.8.1 and TypeScript 6.0.3. That makes a useful prospective comparison environment. These are current lock versions, not verified versions of the earlier evaluation. [JobSys lock](C:/Users/marti/source/JobSys/jobsys-app/bun.lock).

This investigation inspected upstream source and compared file inventories. It did not run the upstream suite or rerun JobSys. The original finding counts remain historical evidence, not measurements of this checkout.

## What upstream actually tests

The plugin package runs Mocha over `tests/src/**/*.ts`, using a compatibility wrapper around ESLint's `RuleTester`. Most rule files call `tester.run(ruleName, rule, loadTestCases(ruleName))`. This is a reusable, file-based corpus, so a Rust runner can consume the cases without translating every TypeScript test. [Test command](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/package.json), [RuleTester wrapper](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/utils/eslint-compat.ts), [example rule test](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/src/rules/no-unused-props.ts).

The authoritative loader is [`tests/utils/utils.ts`](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/utils/utils.ts). Its behavior matters:

- It recursively discovers inputs under `tests/fixtures/rules/<rule>/{valid,invalid}`. It ignores names beginning with `_`, accepts basenames ending in `input`, and also accepts names beginning with `+`, including actual `+page.svelte`, `+layout.svelte` and `+error.svelte` route files. Inputs can be `.svelte`, `.svelte.js`, `.svelte.ts`, `.js` or `.ts`.
- It selects per-file or same-directory `_config` from JSON, JS or CJS. It does **not** inherit arbitrary ancestor `_config` files. Options, settings, language options, parser options and the real filename are part of a case.
- It filters cases using per-file or same-directory `_requirements.json` and installed dependency versions, including Node. `FIXME` entries are comments. Plain `requirements.json` accompanies `+` files because replacing the whole route basename produces that filename.
- Defaults include browser globals, latest ECMAScript, module source type, a shared TypeScript project and TS/JS script parsers. Typed cases need `tsconfig.json` and supporting declarations/imported files, not just the Svelte input.
- Invalid cases read `*-errors.yaml`, with expected message, line, column and suggestion descriptions/outputs. Fixable rules also compare `*-output.<extension>`; unchanged output becomes an expected null fix. The generator applies the collected fixes once to produce that expected output.
- Missing expectations can be generated from the plugin during an upstream test run. A parity importer should fail on missing expectations instead. Otherwise importing an incomplete corpus can silently bless behavior.

Upstream `AGENTS.md` still describes `*-errors.json`, but the loader and checked-in files use YAML at this revision. Implement against the loader. [Agent guidance](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/AGENTS.md), [TypeScript project](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/fixtures/rules/tsconfig.json).

The fixture corpus is not the entire plugin test suite. There are configuration, processor and settings tests, an inline core `no-unused-vars` integration test, and adapted `@typescript-eslint/no-unnecessary-condition` tests. The core integration file has four valid and four invalid store-reference cases. Those are useful for the JS/TS coverage concerns, but the rule fixture import alone will not cover all template scope behavior. [Unused variables integration](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/src/integration/no-unused-vars.ts), [TypeScript integration](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/src/rules/@typescript-eslint/no-unnecessary-condition.ts), [test tree](https://github.com/sveltejs/eslint-plugin-svelte/tree/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/src).

## Inventory against this checkout

Applying the upstream discovery naming rules gives **1,299 raw fixture inputs**, comprising **618 valid and 681 invalid**, across 84 top-level fixture directories. There are 1,281 `.svelte` inputs, 17 inputs ending in `.js` and one ending in `.ts`; 36 use `+` route names. These are raw inputs before dependency filtering, not executed test totals. They exclude inline tests and runtime test expansion. Counts come from the pinned [fixture tree](https://github.com/sveltejs/eslint-plugin-svelte/tree/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/fixtures/rules).

The same inventory method finds 1,285 inputs in local `fixtures/linter`. There are 1,230 shared relative paths; 1,227 have identical content after normalizing CRLF to LF. Upstream has 69 paths absent locally, and local has 55 paths absent upstream. Consequently a sync must preserve local regression cases rather than replace the entire directory. [Local fixtures](../../fixtures/linter), [existing attribution](../../THIRD_PARTY_NOTICES).

| Rule | Upstream valid | Upstream invalid | Local raw inputs | Upstream paths absent locally |
| --- | ---: | ---: | ---: | ---: |
| no-navigation-without-resolve | 42 | 42 | 44 | 42 |
| no-unused-props | 56 | 20 | 76 | 0 |
| prefer-writable-derived | 2 | 7 | 11 | 0 |
| prefer-svelte-reactivity | 20 | 59 | 83 | 0 |
| require-each-key | 2 | 1 | 3 | 0 |
| valid-prop-names-in-kit-pages | 15 | 7 | 18 | 4 |
| no-unused-svelte-ignore | 35 | 20 | 57 | 0 |

The other missing paths belong to `prefer-derived-over-derived-by` with nine, `no-nested-style-tag` with four, `no-at-const-tags` with three, `indent`, `prefer-attribute-interpolation` and `no-bind-value-on-checkable-inputs` with two each, and `mustache-spacing` with one. Three shared inputs differ: `button-has-type/valid/test01-input.svelte` and the Kit rule's `valid/svelte5/+page.svelte` and `valid/svelte5-without-runes/+page.svelte`. Review those differences when synchronizing.

## Why current fixture tests cannot establish parity

The local runner verifies zero target-rule findings for valid cases, and at least one for invalid cases. It does not assert the expected YAML messages, exact finding counts, positions, suggestion outputs or fix output. This can pass while the JobSys example produces a nonsensical unused property name such as `| undefined`. [Fixture runner](../../src/lib.rs).

The runner also skips `typescript` directories, skips `ts` directories for `valid-compile`, misses `-input.svelte.js` and `-input.svelte.ts`, and does not load upstream parser/version requirements. Per-file config naming assumes `-input.svelte`, so ordinary `.js` and `.ts` case configs can be missed. Missing fixture directories produce no cases without a failure. The main mismatch is therefore the expectations and case execution, even for rules whose source inputs are already copied. [Fixture discovery and config](../../src/lib.rs).

Local diagnostics provide byte spans, messages and fixes, but no upstream suggestion or message-ID structure. An adapter should convert byte positions to ESLint-compatible one-based line/column positions, accounting for UTF-16 columns. Keep suggestion parity as a separate reported capability until there is a representation for it. [Diagnostics](../../src/linter/mod.rs).

The real-project parity script compares sets of file, rule and line. It collapses duplicates and does not compare messages, columns or fixes. Keep it as a separate end-to-end signal after the fixture runner becomes precise. [Real-project parity script](../../scripts/parity-real.sh).

## Priority cases and limits

Navigation should go first. Its 42 missing upstream cases cover conditional branches, typed resolved pathnames, string props, wrong-module types and invalid operators. The implementation follows actual `$app/navigation` and `$app/paths` imports, handles aliases and namespaces, allows external/fragment/nullish links, and treats `goto`, `pushState` and `replaceState` differently. It can recognize `$app/types.ResolvedPathname` through TypeScript type services. That is a semantic difference from string matching and may explain disagreements on dynamic expressions. The historical report's broken Markdown is insufficient to decide the individual JobSys cases without their source and config. [Navigation fixtures](https://github.com/sveltejs/eslint-plugin-svelte/tree/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/fixtures/rules/no-navigation-without-resolve), [rule implementation](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/rules/no-navigation-without-resolve.ts).

Unused props should follow immediately. All 76 upstream input paths already exist locally. They include optional props, union/intersection/conditional types, imported types, rest/spread, nesting, index signatures and template usage. Upstream uses the TypeScript type checker and returns no visitors when typed services are unavailable. Accurate messages plus these valid cases will expose false positives that finding-presence tests cannot. The union and optional cases are particularly relevant to the reported parsing bug. [Unused props fixtures](https://github.com/sveltejs/eslint-plugin-svelte/tree/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/fixtures/rules/no-unused-props), [rule implementation](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/rules/no-unused-props.ts).

Kit page props need real route filenames and route-directory settings. The four missing paths test `children` on a page, an invalid error-page prop, valid error-page props and valid layout props. Moving them into generic `*-input.svelte` files would change what the rule tests. [Kit fixtures](https://github.com/sveltejs/eslint-plugin-svelte/tree/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/fixtures/rules/valid-prop-names-in-kit-pages).

Writable derived and Svelte reactivity inputs are already imported, but version/runes conditions and fixes still matter. Require-each-key has only three upstream inputs, so it is a useful early check for the adapter but too small to explain every real-project difference. [Writable derived](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/rules/prefer-writable-derived.ts), [Svelte reactivity](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/rules/prefer-svelte-reactivity.ts), [each-key fixtures](https://github.com/sveltejs/eslint-plugin-svelte/tree/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/tests/fixtures/rules/require-each-key).

Treat unused Svelte ignores separately. Upstream uses Svelte compiler warnings to determine whether an ignore is unused. The current local implementation tracks ignored local lint findings, and the upstream invalid suite is disabled locally. Thus the earlier claim that it is entirely unimplemented is stale, while compiler-backed parity remains a distinct gap. [Upstream ignore rule](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/rules/no-unused-svelte-ignore.ts), [local suppression logic](../../src/linter/mod.rs), [disabled suite](../../src/lib.rs).

Upstream CI tests ESLint 8/9/10 on Linux and Windows, several Node versions, and separate Svelte 3 and Svelte 4 jobs alongside the default Svelte 5 dependency. Start with a declared Svelte 5 target matching JobSys; retain other requirements as explicit skips. Do not label skipping every typed case as suite parity. [CI matrix](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/.github/workflows/NodeCI.yml).

## Recommended implementation sequence

1. Vendor unchanged upstream fixtures and supporting files under a separate pinned directory. Preserve local regression tests. Record repository URL, commit, hashes and license, and generate a manifest containing rule, stable case ID, filename, source, options, settings, parser configuration, dependency requirements, expected diagnostics, fixed output and suggestions. Prefer exporting resolved cases with the upstream loader so config precedence and runtime requirements stay faithful.
2. Build one compatibility runner with the declared environment. Compare exact target-rule diagnostic counts, messages and locations. Compare fixes separately. Report unsupported options, missing rule implementations, parse errors, type-service gaps and version skips explicitly. Missing expectations or undiscovered fixture directories should fail the import, rather than create a vacuous pass.
3. Establish a reviewed baseline of known failures keyed by rule and case. Keep upstream expectations unchanged. CI should reject new mismatches and unexplained skips while reporting progress toward removing the existing gaps.
4. Begin with the three each-key inputs to verify conversion, then navigation's missing 42 paths and the 76 already imported unused-props inputs. Follow with Kit page filenames, writable-derived fixes and reactivity. Add compiler warning parity and the inline/core integration tests as separate work.
5. Rerun JobSys with pinned parser, compiler and rule config after fixture improvements. Keep its real-world disagreement report alongside the stricter upstream fixture results.

This is a proposed implementation plan. This research change adds no runner or rule implementation.

## License

Upstream is MIT licensed with copyright 2021 Yosuke Ota. Its license permits copying and modification provided the copyright and permission notice accompany copies or substantial portions. Oxvelte's `THIRD_PARTY_NOTICES` already contains that notice for imported plugin fixtures. Preserve it and retain attribution/provenance for the pinned import. [Upstream license](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/LICENSE), [local notices](../../THIRD_PARTY_NOTICES).
