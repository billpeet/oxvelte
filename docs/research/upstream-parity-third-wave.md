# Third parallel wave

All three workers finished in isolated worktrees based on `8bac613`. The coordinator implemented the remaining suggestion rules, reviewed the worker changes and integrated each rule with its baseline update. Worker branches and worktrees remain available for inspection.

Results: **946 matches, 289 gaps, 64 version skips**, compared with 863 matches and 372 gaps before this wave. All 83 selected gap cases are closed. The pinned upstream corpus, expectations, manifest and version gates are unchanged.

| Package | Integrated commits | Outcome |
| --- | --- | --- |
| Attribute sorting | `066ca5a` | 26 gaps closed; 43/43 fixtures match. Implements insertion comparison, ordered matching options, spread barriers, attribute locations and rotation edits that preserve whitespace. |
| Handler checks | `f8a0e9d` | 10 gaps closed; 15/15 fixtures match. Reports original expression spans and follows scoped const aliases with cycle protection. |
| Parser-dependent rules | `c2b9534` | Four dynamic-slot gaps closed; 6/6 fixtures match. Reports and fixes individual mustaches, including quoted and concatenated values. Restricted HTML and attribute-equals spacing remain strict at 4/4 and 2/2. |
| Store rules | `6a355d9`, `9cecd3c`, `39d434a`, `49e2521`, `590e40d` | 35 gaps closed. Unsubscribe matches 4/4, callback parameters 4/4, destructured props 14/14, and reactive access 48/48 eligible cases with one existing skip. |
| Other suggestions | `79156e2`, `f8e566a`, `3a3cbd8`, `64c0c66`, `36fa49f` | Eight gaps closed. Debug tags, reactive curlies, reactive functions, reactive literals and event-listener calls all pass strict parity. Optional alternatives remain separate from automatic fixes. |

Reactive store access now checks AST operands rather than source substrings. It reports precise identifier or member spans and prefixes identifiers where upstream permits automatic fixes. Store callback and destructuring alternatives preserve scoped references and shorthand property keys. Review corrections protect nested TypeScript bindings, template locals and inline callbacks, locate parameter insertion at parsed parentheses, and exclude function-local reactive labels from component-level alternatives.

## Validation

All 14 rules in this wave pass strict parity on the combined code. The full Cargo suite passes with 516 library tests, one CLI test, seven parser-contract tests, seven adapter tests and the full parity baseline check. The categorizer's four tests and importer's three tests pass. Formatting and diff checks pass.

The baseline audit confirms exactly 83 removed gap cases, with every retained issue dimension preserving its previous hash. No new or changed remaining issue signatures were introduced. Removed dimensions comprise 67 diagnostic differences, 51 fix-output differences, 17 suggestion differences and 15 diagnostic-count differences. These counts overlap. Corpus identity fields and all 64 version skips are unchanged.

Regressions cover Unicode offsets, quoted script attributes, comments containing punctuation, duplicate and attachment attributes, spread barriers, ordered exclusions, const-alias cycles, template branch scopes, nested typed bindings, callback captures, imported store objects and fix ranges.

## Remaining queue

There are **101 native gap cases** and 188 cases requiring decisions about indentation, TypeScript integration, compiler integration or executable compiler configuration. The [current inventory](upstream-parity-gaps.md) records their first blockers and overlapping issues. The [dispatch manifest](upstream-parity-work-packages.json) remains the original frozen snapshot, so its old blocked statuses and case counts should not be used as live results.

| Remaining native package | Current gaps |
| --- | ---: |
| Kit load exports | 4 |
| Style analysis | 16 |
| Directive preferences | 20 |
| Text layout | 19 |
| Basic reactivity | 10 |
| Environment imports | 11 |
| Slot types | 1 |
| New derived rule | 9 |
| New nested-style rule | 4 |
| New const-tag rule | 3 |
| New checkable-binding rule | 2 |
| New interpolation rule | 2 |
| Total | 101 |

These packages can proceed from the reviewed integration head. New rule registration remains coordinator-owned. Style analysis, directive preferences and text layout are independent candidates for the next parallel wave.

Fixture matches do not establish complete behavior across arbitrary projects. Store type recognition remains a lightweight analysis of annotations, interfaces and imported properties, rather than a complete TypeScript checker. Attribute-order patterns use Rust's regex engine rather than the full JavaScript regex dialect. Template alias handling remains conservative where the app lacks complete scope information. The JobSys comparison has not been rerun.
