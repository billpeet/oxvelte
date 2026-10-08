# Upstream parity progress

The [sixth parallel wave](upstream-parity-sixth-wave.md) is complete and integrated. All 90 eligible compiler/configuration cases now match, bringing the full suite to 1,225 matches, seven gaps and 67 version skips. Remaining work is TypeScript unnecessary-condition integration (7). The earlier wave reports and batch below record previous work.

Remaining work is divided into independent packages in the [parallel work plan](upstream-parity-parallel-plan.md). Its [dispatch manifest](upstream-parity-work-packages.json) retains the original queue snapshot and file ownership; use the current inventory for remaining cases.

Commits 3–8 are complete. They close all 53 targeted gaps against the pinned eslint-plugin-svelte 3.23.0 corpus at revision `18339c886320151148568063c5801bf69cb51027`.

| Result | Before this batch | After this batch |
| --- | ---: | ---: |
| Matching cases | 709 | 762 |
| Gap cases | 526 | 473 |
| Version skips | 64 | 64 |
| Total fixtures | 1,299 | 1,299 |

## Completed commits

| Order | Change | Gaps closed |
| --- | --- | ---: |
| 3 | Inspect nested types on destructured props and apply custom exclusions. | 1 |
| 4 | Report unresolved navigation on the first argument. | 29 |
| 5 | Distinguish nullish values from interpolated strings and retain absolute URL exemptions. | 2 |
| 6 | Check both ternary branches, restrict concatenation to addition, and apply each navigation function's policy. | 10 |
| 7 | Resolve pathname annotations to their imported symbols; check links in files with only type imports. | 7 |
| 8 | Apply separate page, layout and error prop names and match upstream messages. | 4 |

`no-unused-props` now matches all 76 fixtures. `no-navigation-without-resolve` matches all 84. `valid-prop-names-in-kit-pages` matches all 11 eligible fixtures, with its 11 Svelte 3/4 fixtures still skipped by the declared environment. These rules all pass strict mode.

## Gaps after commits 3-8

The groups below assign each gap to its first blocker. Secondary diagnostic, fix and suggestion differences remain recorded in the [full inventory](upstream-parity-gaps.json).

| First blocker | Cases |
| --- | ---: |
| Missing rule implementations | 118 |
| Executable fixture configuration | 6 |
| Svelte compiler capability | 84 |
| Parser/compiler validation differences | 27 |
| Diagnostic count differences | 33 |
| Same count, different messages | 24 |
| Same messages, different locations/order | 105 |
| Fix output or unexpected automatic fixes | 63 |
| Suggestions are the remaining blocker | 13 |
| Total | 473 |

The original native-work division now leaves 285 native gap cases after setting aside 188 cases in indent, TypeScript integration, compiler capability and executable compiler configuration. The [commit plan](upstream-parity-plan.md) retains the remaining work by rule and mechanism.

## Validation and limits

Each fix was checked against the selected upstream rule and the full parity suite before updating its baseline. Existing tests passed after each commit. The final run passed 461 Rust unit tests, four adapter tests and the parity baseline check. Both Node test suites passed, with three tests each. The corpus and expectations were unchanged; only reviewed baseline issue signatures were removed or updated. All 64 version skips remain unchanged.

Regression tests cover argument spans with Unicode and quoted script attributes, nested property options, unsafe ternary branches, non-addition operators, imported type aliases, wrong modules, shadowed type names, nullable/optional pathname values, route-specific props and module-script exclusion.

Matching this fixture suite does not establish complete parity in arbitrary projects. Pathname recognition follows local annotations and imported type identity; it does not implement TypeScript's full structural assignability checker. Unused props still uses the existing limited type extraction. These fixtures also do not cover every template scope or imported type arrangement.

The JobSys comparison has not been rerun in this batch. Its earlier findings should be remeasured after these changes. The [generated gap report](upstream-parity-gaps.md) describes fixture parity, while compiler warnings, suggestions, skipped version environments and tests outside the raw fixture runner remain explicit limitations.
