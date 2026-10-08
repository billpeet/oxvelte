# Upstream parity commit plan

This plan starts from 689 matching cases, 546 gaps and 64 version skips at upstream revision `18339c886320151148568063c5801bf69cb51027`. The [generated inventory](upstream-parity-gaps.md) separates nine first-blocker groups; its [JSON companion](upstream-parity-gaps.json) contains every affected case ID and all secondary issue dimensions.

## What the categories mean

| First blocker | Cases | Work involved |
| --- | ---: | --- |
| Locations/order | 153 | Report the same finding on the same AST node and preserve upstream ordering. Check fixes/suggestions too. |
| Missing rules | 118 | Five missing native Svelte rules account for 20 cases. The other 98 belong to indent or a TypeScript integration rule. |
| Compiler capability | 84 | 46 unused-ignore cases and 38 compiler-validation cases need compiler warning behavior. |
| Fix behavior | 63 | 49 block-lang cases incorrectly offer automatic fixes; ten store-access cases and four other cases have remaining fix differences. |
| Diagnostic count | 56 | Missing or extra findings after excluding parser/compiler blockers. Includes navigation branches, types and nullish links. |
| Parser/validation | 27 | The local parser reports errors on fixtures accepted by the upstream linter parser. Establish the parsing versus compiler-validation contract. |
| Different messages | 26 | Same count, different messages. Some are wording; others encode different calculations or classifications. |
| Suggestions only | 13 | All current diagnostic/fix expectations match, but suggestions are unavailable. Seven are writable-derived. |
| Executable config | 6 | Compiler fixtures use CJS callbacks or other configuration that the importer currently preserves without executing. |

These totals are exclusive. The overlapping measurements are larger: 279 cases have diagnostic differences, 83 have fix-output differences, 54 have unexpected fixes, 78 require suggestions and 29 have parse errors. A case can appear in several of these measurements. A commit is complete only when all targeted issues are resolved, or the remaining ones are identified explicitly.

For native work, 358 gap cases remain after setting aside the current 188 cases in indent, the TypeScript integration rule, compiler capability and executable compiler config. Of those 358, 338 involve existing native rules and 20 involve missing native Svelte rules. This is a scope division for the work queue, not permission to remove expectations or mark deferred cases as passing.

## First commits

Keep these commits separate. Their listed case sets do not overlap, and they give useful progress before adding larger capabilities.

| Order | Commit subject | Targeted cases | Acceptance |
| --- | --- | ---: | --- |
| 1, complete | Fix reactivity checks for default-exported bindings | 1 | `prefer-svelte-reactivity` matches all 79 cases in strict mode. |
| 2, complete | Align unused-props diagnostic locations | 19 | The 19 location-only cases match; all 56 valid cases remain clean. |
| 3, complete | Fix unused-props custom option combinations | 1 | The remaining invalid case reports all three expected findings, at the expected locations. |
| 4, complete | Align navigation diagnostic argument locations | 29 | The 29 location-only navigation cases match, without changing which calls or links are flagged. |
| 5, complete | Fix navigation nullish and literal link handling | 2 | `link-nullish-like-literal01` and valid `link-nullish02` match. |
| 6 | Fix navigation branches and operators | 10 | Eight ternary cases and two invalid-operator cases match. |
| 7 | Recognize resolved pathname types in navigation | 7 | Six valid goto/pushState/replaceState cases and one invalid unresolved-link case match. Check actual imported type identity and scope, rather than accepting a type by its spelling alone. |
| 8 | Correct SvelteKit page, layout and error props | 4 | The four eligible invalid cases match, including children-on-page and error-page behavior. The 11 Svelte 3/4 cases remain explicit version skips. |

The first reactivity case is [`exports02-input.svelte.js`](../../fixtures/upstream/eslint-plugin-svelte/tests/fixtures/rules/prefer-svelte-reactivity/invalid/exports02-input.svelte.js). It constructs a Date, then exports its variable as the default. The expected finding is on the constructor. The implementation now resolves that exported identifier as well as named specifiers. All 79 reactivity cases pass in strict mode. TypeScript exports, shadowed constructors and duplicate named/default exports have regression coverage. This fix removes exactly one gap, bringing the suite to 690 matches, 545 gaps and 64 version skips.

Unused props' last case is [`custom-config-combination-input.svelte`](../../fixtures/upstream/eslint-plugin-svelte/tests/fixtures/rules/no-unused-props/invalid/custom-config-combination-input.svelte). Oxvelte reports two findings where upstream expects three. The 19 other invalid cases now match: root properties, nested properties and index signatures report on the typed `$props()` binding. Unused-property findings precede the index-signature warning at that shared location. Regression tests cover full binding spans, Unicode before the script, quoted `>` attributes and comments before destructuring. This location fix leaves the custom-options missing finding unresolved and brings the full suite to 709 matches, 526 gaps and 64 version skips.

Navigation's typed cases and branches are separate behavior changes. The [JSON inventory](upstream-parity-gaps.json) lists the complete 48-case set. The filename grouping above is a useful proposed split; inspect implementation and expected/actual details before treating each group as a proven shared cause.

## Remaining native commits

After the first eight, proceed by rule or shared mechanism. Do not combine all 153 location cases into one offset adjustment. Different rules intentionally report on different nodes.

| Work | Suggested commit boundaries |
| --- | --- |
| Parser/compiler validation separation | Establish the intended contract first. Then address dynamic slot values, binding/directive validation, and attribute sorting fixtures as distinct changes. There are 27 primary parser cases: 15 sort-attributes, five dynamic-slot, two each in restricted-elements, spacing and store-access, and one handler case. Preserve syntax parse errors. |
| Diagnostic wording and calculations | Separate commits for duplicate on-directives, duplicate use-directives, exported Kit load wording, goto/base wording, block line counts, style-parse messages and reactive-reassign messages. Same counts alone do not establish that the behavior is correct. |
| Remaining locations | One rule per commit where possible. Larger groups include navigation-without-base, handler reports, destructured store props, sort-attributes, class/style directives and store reactive access. Secondary fix/suggestion issues remain visible in the inventory. |
| Suggestions API and comparisons | Add a suggestion representation and exact description/output comparisons first. Then implement suggestions in separate rule commits. There are 78 affected cases in total, including 49 block-lang cases. |
| block-lang fix/suggestion distinction | Replace automatic fixes with upstream suggestions for the 49 affected cases, then resolve its two remaining diagnostic locations. An automatic edit and an optional suggestion are different behavior. |
| Writable derived suggestions | All seven invalid cases already match diagnostics. Implement their expected suggestions after the suggestions API exists. |
| Other suggestion rules | Separate commits for debug tags, extra reactive curls, reactive functions and reactive literals. These account for six suggestion-only cases; other rules have suggestions alongside diagnostic mismatches. |
| Store reactive access | After parser changes, separate finding/location corrections from fix output. Twenty-two cases have fix-output differences, but only ten have fixes as their first blocker. |
| Attribute sorting | After parser changes, fix ordering/report semantics, then edit ranges/output. Twenty-six cases have fix-output differences; the primary groups currently hide these behind parser or diagnostic blockers. |
| Other native behavior | Work rule-by-rule through selector style, duplicate else-if/style properties, reactive reassignment, class/style directives, mustache spacing, shorthand forms and derived inputs/outputs. Full IDs and counts are in the generated inventory. |
| Missing Svelte rules | One commit each: `prefer-derived-over-derived-by` with nine cases, `no-nested-style-tag` with four, `no-at-const-tags` with three, `no-bind-value-on-checkable-inputs` with two and `prefer-attribute-interpolation` with two. |

## Deferred capability decisions

- `indent` accounts for 91 eligible unsupported cases and five version skips. The project currently excludes this formatting rule by design. Keep that limitation explicit until its product scope changes.
- `@typescript-eslint/no-unnecessary-condition` accounts for seven unsupported cases. Decide whether this belongs to the Oxlint/template integration before implementing it as a native Svelte rule.
- Compiler warnings account for 84 primary gaps. An additional six compiler cases use executable configs. Full parity here requires a compiler integration or a separately defined compatibility mode, rather than manufacturing compiler warning expectations.
- The 64 version skips need additional declared environments, chiefly Svelte 3/4 and rules gated to newer Svelte 5 versions. They are not native rule regressions in the current Svelte 5.49.2 environment.
- Custom filesystem, core unused-variable, processor, config and settings tests remain outside this raw fixture runner. Give their imports and runner support separate commits; their absence is not included in the 546-case gap count.

## Verification for each fix commit

1. Run the target rule with a detailed report. Compare the intended case IDs and all their issue dimensions against the current inventory.
2. Run the entire parity suite before changing the baseline. New or changed gaps must be explained and fixed; unchanged known gaps can remain.
3. When the target rule should be fully compatible, also run it with `--strict`. For rules with intentional capability gaps, state precisely what remains.
4. Run the existing relevant tests and `cargo test --locked`. Update the baseline only to remove or review changed issue signatures. Keep the rule fix and its baseline reduction in the same commit.
5. Regenerate the gap inventory and include it in the fix commit so the next commit starts from measured remaining work.

```sh
cargo test --locked --test upstream_parity -- --rule prefer-svelte-reactivity --report reports/reactivity-parity.json
cargo test --locked --test upstream_parity -- --report reports/upstream-parity.json
# After inspecting the full report and resolving regressions:
cargo test --locked --test upstream_parity -- --update-baseline --report reports/upstream-parity.json
node scripts/summarize-parity.mjs
cargo test --locked
```

Treat the generated case inventory as current data. This document records the original commit order and counts; annotate completed batches or revise the order when new evidence changes the diagnosis.

Commit 3 checks named nested types on destructured props, including imported types when enabled, and applies nested type/property exclusions. All 76 unused-props cases now pass in strict mode. The full suite has 710 matches, 525 gaps and 64 version skips.

Commit 4 reports navigation findings on the first argument, including aliases and namespace calls. All 29 location-only cases now match. The 19 remaining navigation gaps retain their finding counts; their script spans also received this correction. The full suite has 739 matches, 496 gaps and 64 version skips.

Commit 5 distinguishes empty strings from unknown template prefixes and reads null/undefined annotations on destructured bindings. Scheme-bearing template quasis keep their upstream absolute-URL exemption. Both targeted cases match, with all previously matching navigation cases preserved. The full suite has 741 matches, 494 gaps and 64 version skips.
