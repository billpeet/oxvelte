# Second parallel wave

All three workers finished in isolated worktrees based on `3de68aa`. Their changes are reviewed, integrated and committed on `polycode/muyz9yt3`. Worker branches and worktrees are retained for inspection.

Results: **863 matches, 372 gaps, 64 version skips**, compared with 789 matches and 446 gaps before this wave. This closes 74 gap cases. The pinned upstream corpus, expectations, manifest and version gates are unchanged.

| Package | Integrated commits | Outcome |
| --- | --- | --- |
| Block language | `e0f9853`, `fcc5a70`, `32d5383` | 51 gaps closed; all 89 cases match. Aligns options, messages and locations, replaces automatic fixes with exact upstream suggestions, and keeps missing-block spans within UTF-8 source boundaries. |
| Rune suggestions | `9df88a2`, `10ee2bc`, `6beaee1`, `36997b7` | 12 gaps closed. Writable derived matches 9/9, unnecessary state wrap 9/9, and derived callback names 2/2. Suggestions use imported symbol identity and preserve source text, scopes and shorthand property keys. |
| Parser foundation | `6f42fdb`, `1fd6940` | 11 gaps closed and 28 parser-error dimensions removed. Lint parsing retains syntax errors while allowing compiler-invalid constructs to reach rules. Directive subjects retain `$` identifiers and their spans. |
| Shared duplicate-directive helper | `4c13960` | Corrects additional behavior outside the pinned gap inventory. Token comparison preserves operator boundaries and literal contents while ignoring whitespace and comments. |

## Parser contract

Public `parse()` and the CLI `parse` command retain compiler-oriented validation. New `parse_for_lint()` is used by CLI lint and the parity runner. It bypasses named compiler analysis passes, rather than filtering diagnostic strings, and preserves structural and expression syntax errors. Existing CLI handling of returned parser errors is unchanged.

The parser changes close two mustache-spacing cases, one dynamic-slot case, one handler case, two restricted-element cases, two attribute-spacing cases, two attribute-sort cases and one store-access case. Other formerly blocked fixtures retain their existing rule differences. The remaining parser-error fixture uses Babel function-bind syntax and requires unsupported script parser configuration.

## Validation

The combined Cargo suite passes: 488 library tests, one CLI test, seven parser-contract tests, seven adapter tests and the full parity baseline check. The four completed block/rune rules pass strict parity, as does attribute spacing. The categorizer's four tests and importer's three tests pass. Formatting and diff checks pass.

The baseline audit confirms exactly 74 removed gap cases. Every retained issue dimension has its previous hash; no new or changed issue signatures were introduced. Removed dimensions across all cases comprise 61 suggestion differences, 49 unexpected automatic fixes, 28 parser errors, nine diagnostic differences, three diagnostic-count differences and two fix-output differences. These counts overlap and do not sum to 74. All 64 version skips and corpus identity fields are unchanged.

Regression tests cover Unicode prefixes, quoted `>` in script attributes, comment preservation, imported aliases and namespaces, shadowed bindings, reassignment, array callback parameters, shorthand properties, capture through default parameters, syntax recovery and directive identifier spans.

## Remaining work

There are 184 native gap cases and 188 cases requiring decisions about indentation, TypeScript integration, compiler integration or executable compiler configuration. The current [gap inventory](upstream-parity-gaps.md) records their exact first blockers and overlapping issues. The [dispatch manifest](upstream-parity-work-packages.json) remains the original frozen queue snapshot; use fresh reports when assigning the next wave.

`foundation-parser`, `block-language` and `rune-suggestions` are complete. Parser-dependent packages can now proceed from the reviewed integration head: attribute sorting, handler checks, store rules, dynamic slots and restricted elements. Other independent packages remain available in the [parallel plan](upstream-parity-parallel-plan.md).

Missing-block diagnostics retain upstream's synthetic column 2 for ASCII and BMP Unicode. The byte-span API cannot express that column for empty sources or an initial astral character; those use valid columns 1 and 3 respectively. Derived rename suggestions are withheld when they would capture another binding. Fixture parity does not establish complete behavior across arbitrary projects, and the JobSys comparison has not been rerun.
