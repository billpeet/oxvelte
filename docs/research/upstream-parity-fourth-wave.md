# Fourth parallel wave

The three workers and coordinator completed the 101-case native queue in isolated worktrees based on `369738e`. Changes are integrated in 35 implementation, prerequisite and review commits, with separate commits for each rule. Worker branches and worktrees remain available for inspection.

Results: **1,044 matches, 188 gaps and 67 version skips**, compared with 946 matches, 289 gaps and 64 skips before this wave. **98 former gaps now match; three const-tag cases are correctly version-skipped.** No native fixture gaps remain in the declared environment.

| Package | Former gaps | Outcome and integrated commits |
| --- | ---: | --- |
| Kit load exports | 4 | Closed. Module/instance export messages, route filenames and source locations: `b88bb03`. |
| Style analysis | 16 | Closed. Selector preferences, CSS alternative sets, property spans, parse messages and optimized style locations: `54814a0`, `c5433fa`, `bf2aabc`, `afa57d7`, `99f29b8`, `12d21d9`, `c37fefb`. |
| Directive preferences | 20 | Closed. Class boundary analysis, operator inversion and CSS conversion with safe source edits: `b2bcc91`, `b98ea12`. |
| Text layout | 19 | Closed. Mustache locations and fixes, shorthand values, each-key references, block line counts and closing tags: `dfafe44`, `f49ae86`, `22302ba`, `5eb312b`, `a3d5a2e`, `09326ce`, `99d55dd`, `d17dbea`, `d97ac2d`. |
| Basic reactivity | 10 | Closed. Reactive operation spans and scoped writes, logical duplicate conditions, and corrected inherited prefer-const metadata: `2705fdd`, `537f1c6`, `f4009a7`. |
| Environment imports | 11 | Closed. Import declarations, optional DOM call locations, browser guards and global-object reads: `d49495d`, `840c97f`, `777aa60`. |
| Slot types | 1 | Closed. Program location and effective script language: `4f35e3e`. |
| Derived callbacks | 9 | Closed. New rule with script/template source spans and callback exclusions: `e106530`, `df81498`. |
| Nested style tags | 4 | Closed. New rule distinguishes template styles from the root stylesheet: `d7c0cc5`. |
| Const-tag migration | 3 | Reclassified as version skips. Parser support, release safeguard and new migration rule: `ccb3954`, `243cb9a`, `982e9be`; importer gate correction: `f4009a7`. |
| Checkable input bindings | 2 | Closed. New rule with separate checked/group suggestions and scoped const type resolution: `08305cd`. |
| Attribute interpolation | 2 | Closed. New rule preserves comment, escape and multiline exclusions: `17ea7ff`. |
| Total | 101 | 98 additional matches and three corrected skips. |

The shared review correction `132658a` preserves TypeScript assertion guards across object interpolation, derived callbacks, checkable inputs and attribute interpolation. Additional review corrections are included in the relevant rule commits. They protect HTML entities and Unicode spans, locate equality operators outside comments, inspect every possible neighboring class branch, preserve CSS attribute delimiters and JavaScript escapes, and retain derived callbacks with directive statements or explicit TypeScript `this` parameters.

## Importer corrections

The importer previously recognized fixability only from a literal property in a rule source. Upstream [prefer-const](../../fixtures/upstream/eslint-plugin-svelte/src/rules/prefer-const.ts) spreads ESLint's core metadata instead. The importer now inspects TypeScript syntax and resolves that inherited metadata from the exact declared ESLint version. All seven prefer-const cases receive the correct fixability flag; its three invalid cases load their existing, unchanged output companions. The implementation already produced the expected fixes.

Upstream [no-at-const-tags](../../fixtures/upstream/eslint-plugin-svelte/src/rules/no-at-const-tags.ts) has a runtime compiler gate requiring Svelte `>=5.56.0`, even though its fixture requirements only say `>=5.0.0`. The declared comparison environment remains Svelte 5.49.2. The importer now recognizes a statically proven compiler-version guard returning no listeners, making all three cases ineligible under that environment. Their original requirements, diagnostics and fix outputs are preserved. These cases are not counted as matches.

The migration itself is implemented and tested independently. It runs in runes mode when declared dependency ranges guarantee Svelte 5.56 or later, removes the legacy marker and wraps the initializer in `$derived(...)` unless already wrapped. Unknown versions and ranges allowing older releases suppress the migration. The parser accepts replacement declaration tags with correct spans and multiple declarators. Modern tags also allow root and regular-element placement, following Svelte's [DeclarationTag visitor](https://raw.githubusercontent.com/sveltejs/svelte/svelte@5.56.0/packages/svelte/src/compiler/phases/2-analyze/visitors/DeclarationTag.js); legacy placement checks remain, following the [ConstTag visitor](https://raw.githubusercontent.com/sveltejs/svelte/svelte@5.56.0/packages/svelte/src/compiler/phases/2-analyze/visitors/ConstTag.js).

Stylesheet parser messages contain filenames. The parity adapter now maps only the evaluated file's absolute or cwd-relative spelling to its original upstream filename when comparing diagnostic messages and suggestion descriptions. It preserves the absolute lint path for package and route resolution. Boundary tests prevent changes to other filenames, longer paths or ordinary message text; expected snapshots are untouched.

## Validation and audit

All 28 eligible rules in this wave pass strict parity on the combined code. The const-tag rule has three explicit version skips and seven native parser, migration and release-guard regression tests. The full Cargo suite passes with **558 library tests, one CLI test, seven parser-contract tests and eight adapter tests**, plus the full parity baseline check. The importer's five tests, categorizer's four tests, formatting and diff checks pass.

The baseline audit against `369738e` confirms exactly **98 removed gap cases** and **three unsupported-rule cases reclassified as version skips**. Every other retained issue hash is unchanged, with no added cases. All 64 previous skips remain unchanged. The pinned revision, declared environment and SHA-256 values for all **2,824 corpus files** are unchanged.

The manifest changes only ten cases: seven prefer-const fixability records, including three existing output companions, and three const-tag eligibility records. This reviewed metadata correction changes the manifest identity and its baseline identity together. No upstream source, fixture, requirement or expected diagnostic/fix bytes were rewritten.

## Remaining decisions

| Work | Gap cases |
| --- | ---: |
| Indentation implementation | 91 |
| Unused Svelte ignores and compiler warnings | 46 |
| Compile validation and executable compiler configuration | 44 |
| TypeScript unnecessary-condition integration | 7 |
| Total | 188 |

The compiler-related work totals 90 cases; six of those require executable fixture configuration. The [current inventory](upstream-parity-gaps.md) records all case IDs and overlapping issues. The [dispatch manifest](upstream-parity-work-packages.json) remains the original queue snapshot and should not be used for live counts.

Fixture matches do not establish full parity across arbitrary projects. CSS analysis uses the app's parser and does not supply every upstream preprocessor. Const type resolution is conservative and is not a complete static evaluator or TypeScript checker. Minimum-release detection uses declared package ranges rather than the installed compiler. Modern declarations retain the native `ConstTag` representation; compiler-compatible `DeclarationTag` serialization and full compiler semantics remain outside this wave. The JobSys comparison has not been rerun.
