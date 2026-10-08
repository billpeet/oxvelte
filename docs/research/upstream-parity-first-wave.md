# First parallel wave

All three workers finished in isolated worktrees based on `11e9737`. The coordinator reviewed and integrated their changes, regenerated the baseline after each logical fix, and validated the combined code.

Current results: **789 matches, 446 gaps, 64 version skips**. This wave closes 27 gap cases. Upstream fixture blobs, expectations, manifest and version gates are unchanged.

| Package | Worker commits | Integrated commits | Outcome |
| --- | --- | --- | --- |
| Duplicate directives | `eec96bd`, `70659c4` | `f91d6fe`, `3bb8901` | Nine gaps closed. Event directives match all seven cases; action directives match all six. |
| Suggestions foundation | `4e556f2` | `4b7f328` | Suggestions API, CLI serialization and exact per-diagnostic comparisons implemented. All 78 capability markers migrated to concrete suggestion mismatches; gap count unchanged by this foundation. |
| Navigation base | `6509ea1`, `d1ffddd`, `98db9ca` | `bd765ba`, `19598cf`, `09c3beb` | Eighteen gaps closed. Legacy goto matches all six cases; navigation base matches all 35. Review also corrected quoted-script-attribute offsets. |

The four rules pass strict mode. The full Rust suite passes with 471 library tests, one CLI serialization test, seven adapter tests and the parity baseline check. The categorizer's four tests and importer's three tests pass. Formatting and diff checks pass. Baseline audits confirm exactly 27 removed cases and exactly 78 migrated suggestion dimensions, with every other retained issue signature unchanged.

Suggestions stay attached to their diagnostics and preserve alternative ordering. Each alternative is applied independently to the original source when compared with upstream output. Optional suggestions do not become automatic fixes. Rule owners can construct `Suggestion { description, fix }` and call `diagnostic_with_suggestions`; multiple edits in one alternative must be normalized to one encompassing replacement.

## Remaining queue

The [dispatch manifest](upstream-parity-work-packages.json) remains a frozen snapshot of the starting queue. The packages `navigation-base` and `duplicate-directives`, plus `foundation-suggestions`, are complete. New worktrees should use the reviewed integration head rather than the original base when they need this API.

There are now 258 native gap cases and 188 capability-decision cases. Of the native cases, 174 can proceed with the current foundations: 103 cases in the untouched ready packages and 71 in block language, rune suggestions and other suggestions. The other 84 remain in parser-dependent packages, including store rules. The parser foundation is the next shared prerequisite. The 64 version skips remain separate.

## Shared helper follow-up

This follow-up was completed as `4c13960` in the [second wave](upstream-parity-second-wave.md). The findings below record the original issue.

The duplicate-directive worker found additional behavior outside the pinned gap inventory. `directive_expression_key` in `src/linter/rules/mod.rs` removes whitespace without preserving token boundaries, and its quote copier stops at the opening quote. These pairs can incorrectly receive the same key:

- `on:click={() => x++ + y}` and `on:click={() => x + ++y}`.
- `use:foo={"a b"}` and `use:foo={"ab"}`.

Keep this as a separate shared-helper fix with regressions for both consumers. Reserve `src/linter/rules/mod.rs` with the coordinator because new-rule registration also uses that file. These examples are additional findings, not cases silently added to or removed from the 446-gap corpus count.

All first-wave worker branches and worktrees are retained for inspection. The JobSys comparison has not been rerun.
