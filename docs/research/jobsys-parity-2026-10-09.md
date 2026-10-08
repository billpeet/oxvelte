# JobSys parity check, 9 October 2026

The [follow-up fixes](jobsys-parity-fixes-2026-10-09.md) close the real rule gaps described below. One correction to this initial interpretation: the unmatched mutable-set entry is a duplicate ESLint diagnostic. Oxvelte already reports the same constructor once.

The upstream fixture suite passes, but JobSys still exposes rule gaps. On files both tools can parse, ESLint reports 1,030 Svelte diagnostics and Oxvelte reports 1,311. Exactly 1,028 agree on file, rule, complete source range, message and severity. There are two ESLint-only diagnostics and 283 Oxvelte-only diagnostics.

Of the extras, 231 are a version gate difference: eslint-plugin-svelte 3.23.0 runs `no-navigation-without-resolve` only on SvelteKit 1/2, while JobSys now uses SvelteKit 3. Oxvelte still runs it. Setting those navigation diagnostics aside leaves 52 Oxvelte-only diagnostics and two misses. This adjustment describes the gate difference; it does not establish whether those links are correct under Kit 3.

The [full result JSON](jobsys-parity-2026-10-09.json) retains all differences, per-rule counts, the excluded file and three reduced examples.

## Environment and method

JobSys was tested at `1713a95ce76e81b4e9873fd03fa4c04504ed0f24`, with a clean working tree, using the release build of Oxvelte at `369503f02e2afa81b40edd3010230c3a551d2c03`. The build succeeded with two existing Rust warnings.

The initial installed dependencies were stale and ESLint could not import `@sveltejs/load-config`. Running `bun install --frozen-lockfile --ignore-scripts` refreshed them without changing tracked JobSys files. The measured installed versions are ESLint 10.9.1, eslint-plugin-svelte 3.23.0, Svelte 5.57.2, SvelteKit 3.0.1, TypeScript 6.0.3, typescript-eslint parser 8.70.0 and svelte-eslint-parser 1.8.1. These versions differ from the earlier evaluation, particularly SvelteKit.

ESLint used JobSys's actual config and ignore rules over all component and Svelte script module globs. It evaluated 990 files: 976 `.svelte` components and 14 `.svelte.ts` modules. Each file's resolved Svelte rule severities, options and settings were transferred to Oxvelte. They formed one config group. Native rules outside `svelte/*` were disabled, and the resolved Svelte compiler async option was preserved. Oxvelte resolved compiler dependencies from JobSys itself.

Counts preserve duplicate diagnostics rather than collapsing findings on the same line. Exact agreement includes start and end locations, message and severity. Fixes and suggestions were not compared. This is a Svelte rule comparison; oxlint and the general JS/TS rules were not evaluated.

## Findings

The table excludes `AttachmentGallery.svelte`, where ESLint has a fatal parser error.

| Rule | ESLint | Oxvelte | Exact matches | ESLint only | Oxvelte only |
| --- | ---: | ---: | ---: | ---: | ---: |
| require-each-key | 735 | 735 | 735 | 0 | 0 |
| prefer-svelte-reactivity | 181 | 180 | 180 | 1 | 0 |
| no-navigation-without-resolve | 0 | 231 | 0 | 0 | 231 |
| prefer-writable-derived | 36 | 36 | 36 | 0 | 0 |
| no-unused-svelte-ignore | 39 | 39 | 39 | 0 | 0 |
| no-unused-props | 2 | 49 | 1 | 1 | 48 |
| valid-prop-names-in-kit-pages | 2 | 2 | 2 | 0 | 0 |
| no-inner-declarations | 0 | 3 | 0 | 0 | 3 |
| system | 0 | 1 | 0 | 0 | 1 |

All findings for `no-at-html-tags`, `no-dom-manipulating`, `no-inspect`, `no-unnecessary-state-wrap`, `no-useless-children-snippet` and `no-useless-mustaches` match exactly. Together those rules contribute 35 matches. Enabled rules with no findings are not listed.

### Unused props

The 48 Oxvelte-only diagnostics span 18 files. They fall into these investigation groups:

| Pattern | Extras | Evidence |
| --- | ---: | --- |
| Nested-property consumption and aliasing | 31 | `ActionMenu.svelte` reports six fields of `items` as unused despite filtering and rendering them through `visibleItems` and an each alias. Other cases pass nested objects to child components or helpers. |
| Multiline union parsing | 11 | Function type unions yield fictitious property names such as `|` and `| undefined`. A small standalone example reproduces both diagnostics. |
| Destructuring defaults | 3 | `EBars.svelte` invents nested property names from `size = 'md'`, `animated = false` and the renamed `class` binding. |
| Index-signature scanning | 1 | `EquipmentMappingPopup.svelte` receives an unused index-signature diagnostic even though its `Props` interface contains no index signature. The current implementation scans beyond the interface's closing brace. |
| Used callback bindings | 2 | `VariationModal.svelte` flags `onSave`, and `EditVariationPopup.svelte` flags `onSaved`. The callbacks are used in their components. |

Oxvelte also misses the unused `getId` declaration in `ContentEditableWithAsyncSearch.svelte`. ESLint reports the full props declaration at lines 25–41. The property appears in the interface but is absent from the destructuring.

These results show that matching the imported unused-prop cases did not cover the type and consumption patterns present in this app. The nested-property group still needs case-by-case reductions before choosing the implementation fix.

### Version gates and rule options

Navigation's upstream `meta.conditions` explicitly accepts Kit versions `1.0.0-next`, `1` and `2`. Its wrapper therefore returns no listener under installed Kit 3.0.1, despite the rule being enabled in the resolved config. Oxvelte emits 234 findings overall, including three in the reference parser failure file. The comparable count is 231. This needs a Kit version gate decision before further navigation semantic comparisons.

All three `no-inner-declarations` extras occur in `CadSysEntityCanvas.svelte`. JobSys's resolved options include `blockScopedFunctions: "allow"`. Oxvelte's implementation explicitly treats this option as `"disallow"`; a small conditional function declaration reproduces the disagreement. This is an option-handling gap, rather than a disabled-rule config discrepancy.

### Reactivity and parser validation

ESLint emits two identical diagnostics for `new Set<string>()` at line 41 in `src/lib/url-params.svelte.ts`; Oxvelte emits one. The initial analysis incorrectly described the unmatched duplicate as a miss. A reduced exported-function example confirms the duplicate reference behavior. Oxvelte retains one diagnostic for that constructor.

Oxvelte reports a nested `<style>` in `src/routes/setups/checklist-categories/+page.svelte` at lines 269–279. ESLint accepts it, and compiling the actual component with Svelte 5.57.2 succeeds without warnings. A minimal `<div><style>...</style></div>` also reproduces the Oxvelte-only system diagnostic. This is a parser validation false positive.

ESLint still fails on `AttachmentGallery.svelte` at line 331, column 11, with `Expected JS Identifier, or MemberExpression, but CallExpression found.` Oxvelte evaluates the file and reports seven findings: two each-key, one writable-derived, one reactivity and three navigation findings. They are retained in the raw result but excluded from the comparable counts. Raw totals are 1,030 ESLint findings and 1,318 Oxvelte findings.

## Reproduction and next work

From the Oxvelte repository, after installing the project's locked dependencies:

```sh
cargo build --release --locked
node scripts/parity-project.mjs C:/Users/marti/source/JobSys/jobsys-app
```

The script writes raw outputs, resolved configs, batch status records, summary and differences under `reports/parity-project/jobsys-app`. `--eslint-only` prepares the reference results; `--reuse-eslint` reruns Oxvelte against saved reference results. Reuse is appropriate only while the project sources, dependencies and config remain unchanged. The existing testbed parity script disables several now-supported rules, so it was not used for this check.

The reference pass took about 279 seconds. Oxvelte took about eight seconds across 29 separate processes. These are local run durations, not controlled benchmark results. Repeating Oxvelte through the saved reusable runner reproduced every comparison count, with no batch errors or stderr output. JobSys's tracked files remained unchanged.

The follow-up work covers Kit 3 navigation eligibility, multiline unused-prop unions, unused-prop destructuring and index-signature bounds, nested-property consumption, block-scoped function options and nested style validation. The reactivity entry needs a duplicate-reference regression rather than a missing-finding fix. Each regression lives outside the frozen upstream corpus. The full upstream corpus and primary baseline were not changed by this evaluation.
