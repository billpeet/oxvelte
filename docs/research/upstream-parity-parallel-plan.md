# Parallel parity work plan

Status: the [fifth wave](upstream-parity-fifth-wave.md) is complete, following the [fourth](upstream-parity-fourth-wave.md), [third](upstream-parity-third-wave.md), [second](upstream-parity-second-wave.md) and [first](upstream-parity-first-wave.md) waves. Current results are 1,135 matches, 97 gaps and 67 skips. All 91 eligible indentation cases now match. Remaining packages cover compiler integration/configuration (90) and TypeScript unnecessary-condition integration (7). The tables and dispatch manifest below retain the original queue snapshot; use the wave reports and current inventory for live counts.

The code starting point is `aa9b3ea`, with 762 matches, 473 gaps and 64 version skips. The [dispatch manifest](upstream-parity-work-packages.json) assigns every remaining gap case to exactly one package and records its rules, source files and dependencies. It is a snapshot of this starting point; use fresh reports to assess a package after prerequisites merge.

Split ownership by rule. Location, count, fix and suggestion differences in the same rule stay with one owner. Each owner should make separate commits for distinct fixes or rules within their package. The counts below are assigned gap cases, not a promise that one change will resolve them all.

## Shared foundations

| Package | Contract and ownership | Downstream work |
| --- | --- | --- |
| `foundation-suggestions` | Add suggestions to `LintDiagnostic` and its constructors, expose them where diagnostics are serialized, and compare each suggestion's description and independently applied output in the parity runner. Own `src/linter/mod.rs`, `src/main.rs`, `tests/support/parity.rs`, `tests/upstream_parity.rs`, `tests/upstream_adapter.rs` and categorizer changes. Preserve diagnostic association and ordering. | Block language, rune suggestions, other suggestions and parts of store rules. There are 78 cases with suggestion expectations across these packages. |
| `foundation-parser` | Establish the syntax-error versus compiler-validation contract. Own `src/parser/` and necessary AST changes, chiefly `src/parser/template.rs` and `src/parser/mod.rs`. Preserve genuine syntax errors and the public parser behavior required by existing consumers. Add local parser regressions under this ownership. | Attribute sorting, handler checks, store access, dynamic slots, restricted elements and attribute spacing. There are 27 primary parser blockers, plus secondary parser issues. |
| `foundation-compiler` | First research how the runtime will obtain compiler warnings, handle suppression and unused ignores, and execute supported fixture configuration. Then own the implementation across the two compiler rules, suppression logic, runtime dependencies and runner capability gates. | 90 cases in the compiler package. Implementation waits for the suggestions and parser contracts, plus the compiler integration decision. |

Foundation packages have no exclusive case count. Their effects overlap rule packages. The parser owner must reserve any necessary `src/lib.rs`, `src/linter/mod.rs` or runner changes with the coordinator. It can investigate and edit parser-owned files while suggestions work runs, but shared-file changes must merge sequentially. Compiler work also touches the diagnostic API and runner, so it follows the suggestions foundation.

Do not remove capability markers or suppress parse errors merely to make tests pass. A foundation must implement and test the behavior behind its changed comparison or parser contract. Removing a parser blocker may reveal diagnostic or fix differences that remain assigned to the rule owner.

## Packages ready to start

These packages cover 130 gaps. Their rule modules do not overlap. Owned filenames and exact case IDs are in the dispatch manifest.

| Package | Rules | Cases |
| --- | --- | ---: |
| `navigation-base` | `no-navigation-without-base`, `no-goto-without-base` | 18 |
| `duplicate-directives` | `no-dupe-on-directives`, `no-dupe-use-directives` | 9 |
| `kit-load-exports` | `no-export-load-in-svelte-module-in-kit-pages` | 4 |
| `style-analysis` | Selector style, duplicate style properties, shorthand overrides, unknown style properties, style parsing and optimized style attributes | 16 |
| `directive-preferences` | `prefer-class-directive`, `prefer-style-directive` | 20 |
| `text-layout` | Mustache spacing, closing-bracket newline, block line counts, objects in text, useless mustaches, each keys and attribute/directive shorthand | 21 |
| `reactivity-basic` | `no-reactive-reassign`, `prefer-const`, `no-dupe-else-if-blocks` | 10 |
| `environment-imports` | DOM manipulation, Svelte internal imports and top-level browser globals | 11 |
| `slot-types` | `experimental-require-slot-types` | 1 |
| `new-derived-rule` | Implement `prefer-derived-over-derived-by` | 9 |
| `new-nested-style-rule` | Implement `no-nested-style-tag` | 4 |
| `new-const-tag-rule` | Implement `no-at-const-tags` | 3 |
| `new-checkable-bind-rule` | Implement `no-bind-value-on-checkable-inputs` | 2 |
| `new-interpolation-rule` | Implement `prefer-attribute-interpolation` | 2 |

Existing-rule owners keep regressions in their rule's test module or a uniquely named integration test. They may read all source and upstream fixtures, but must ask the coordinator to reserve changes outside their owned files. A shared helper discovered during implementation becomes a separate prerequisite rather than an edit duplicated across branches.

The five new rules can be developed independently. All need `src/linter/rules/mod.rs` registration. Workers may make a separate, minimal registration commit locally to run their tests. The coordinator integrates those declarations and `all_rules()` entries serially. No worker should refactor the registry or change another rule's registration. Include any required upstream options and recommended status, and test both valid and invalid fixtures.

## Packages after foundations

These packages cover another 155 gaps. An owner may inspect fixtures early, but should start implementation from a commit containing its prerequisites. Do not declare the whole package complete while secondary fix or suggestion issues remain.

| Package | Rules | Cases | Depends on |
| --- | --- | ---: | --- |
| `handler-checks` | `no-not-function-handler` | 11 | Parser contract |
| `store-rules` | Reactive store access, destructured store props, callback set parameter and ignored unsubscribe | 36 | Parser contract and suggestions API |
| `attribute-sorting` | `sort-attributes` | 28 | Parser contract |
| `parser-dependent-rules` | Dynamic slot names, restricted HTML elements and spaces around attribute equals | 9 | Parser contract |
| `block-language` | `block-lang` | 51 | Suggestions API |
| `rune-suggestions` | Writable derived, unnecessary state wrapping and derived inputs/outputs | 12 | Suggestions API |
| `other-suggestions` | Debug tags, extra reactive curls, reactive functions/literals and addEventListener | 8 | Suggestions API |

Store access and attribute sorting each need several commits: diagnostic semantics and locations first, then edit ranges and fix output. The store package has both parser and suggestion prerequisites because its rules share related behavior; one owner avoids conflicting follow-up work. The parser-dependent package owns its rule files, while the parser foundation retains parser files.

## Separate capability decisions

These packages account for the remaining 188 gaps. Research can run alongside native fixes, with notes confined to a unique research file for each decision.

| Package | Cases | Required decision |
| --- | ---: | --- |
| `compiler-rules` | 90 | `decision-compiler`: runtime/compiler integration, suppression and executable-config contract. Includes 46 unused-ignore cases, 38 compiler-capability cases and six executable configurations. |
| `indent-scope` | 91 | `decision-indent`: whether formatting belongs in this app. It is currently excluded by design. Keep expectations visible unless scope changes. |
| `typescript-integration` | 7 | `decision-typescript`: whether and how template-aware TypeScript checks belong in the Oxlint integration. A native name-based approximation is insufficient for type-aware expectations. |

The 64 version skips are a separate environment-matrix project and are not included in these 473 cases. Additional processor/config/settings and core-rule test ports also remain outside this raw fixture count. Give those runner extensions separate ownership after the foundation changes settle.

## Worktree and scheduling protocol

Use one branch and worktree per active package. The coordinator remains in the current worktree. With four agent slots, run at most three workers alongside it. Suggested first dispatch: suggestions foundation, navigation base and duplicate directives. Start the parser foundation when a worker finishes, then release its dependent packages as the contract lands. Fill other slots from the ready queue while foundations are in progress.

For a ready package, the coordinator can create its worktree with PowerShell:

```powershell
$parityRepo = 'C:\Users\marti\source\oxvelte-worktrees\muyz9yt3'
$parityTree = 'C:\Users\marti\source\oxvelte-worktrees\parity-navigation-base'
git -C $parityRepo worktree add -b parity/navigation-base $parityTree aa9b3ea
```

For a dependent package, replace the base with the reviewed integration commit containing its prerequisites. Record that exact commit in its assignment. Use the worktree's own `target` directory and `reports` directory. Do not share a writable Cargo target directory between workers.

Spawned agents inherit a working directory, so the assignment must explicitly name the worktree. Every shell call must use that directory, and every file edit must use an absolute path inside it. Workers must not run checkout, reset, cherry-pick or writes in the coordinator's worktree or another worker's worktree. Creating the worktree precedes dispatch; naming a branch in a prompt does not create isolation.

## Shared artifacts and integration

The coordinator alone owns `tests/upstream-parity-baseline.json`, generated gap inventories, and shared progress/plan documents. Workers leave these unchanged in their commits. They also leave upstream corpus files, the manifest, dependency gates and expected snapshots unchanged.

Workers return implementation commit hashes, target-rule reports, a full-suite report, test results and any unresolved case IDs. Known-gap disappearance is already accepted by the runner. A changed known-gap signature can fail its baseline check; inspect and explain that difference rather than resetting the baseline. Run strict mode for each rule expected to be fully compatible. If it still has a foundation or capability blocker, report that explicitly.

```powershell
cargo test --locked --test upstream_parity -- --rule no-navigation-without-base --report reports/navigation-base.json
cargo test --locked --test upstream_parity -- --report reports/upstream-parity.json
cargo test --locked
cargo fmt --check
git diff --check
```

For each logical worker commit, the coordinator applies it with `git cherry-pick --no-commit`, resolves only reviewed registration conflicts, and runs the target and full suites on the combined code. After reviewing all new/changed issue dimensions, it regenerates the full baseline and inventories, then commits the implementation and its baseline changes together. This preserves the earlier one-fix-per-commit workflow without concurrent generated-file edits. Do not copy a worker's entire baseline over the integrated baseline.

Accept a package when all assigned eligible cases match in every dimension, previously passing cases remain passing, expected skips remain explicit, and relevant regressions and the full suite pass. Partial delivery is useful, but list every remaining case and dependency rather than marking it complete. Review spans, ordering, automatic fixes and suggestions separately.

## Worker assignment template

```text
Package: <id from upstream-parity-work-packages.json>
Worktree: <absolute path already created by the coordinator>
Branch/base: <branch> at <exact reviewed commit>
Objective: Resolve the assigned cases for the listed rules, including all
secondary dimensions. Read the corresponding pinned upstream rule and fixtures.
Ownership: Only the package's ownedFiles and uniquely named regression tests.
Prerequisites: <merged foundation commits, or none>.
Use this worktree for every tool call and absolute edit path. Do not modify
upstream expectations, the parity baseline, generated inventories or shared docs.
Do not add rule-specific exceptions to the runner. Preserve passing cases.
Commit distinct fixes separately. Return commit hashes, report paths, test results,
resolved/remaining case IDs and any shared-file changes needed from the coordinator.
```

The dispatch contract is established. First-wave workers ran in three separate worktrees; their delivery and integration are recorded in the first-wave report.
