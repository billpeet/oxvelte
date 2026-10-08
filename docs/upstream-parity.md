# Upstream rule parity

Install the pinned compiler runtime before running the fixture suite:

```sh
npm ci --prefix scripts/compiler-runtime --ignore-scripts --no-audit --no-fund
```

Set `OXVELTE_COMPILER_RUNTIME` to that directory. In PowerShell:

```powershell
$env:OXVELTE_COMPILER_RUNTIME = (Resolve-Path scripts/compiler-runtime).Path
```

On Linux or macOS:

```sh
export OXVELTE_COMPILER_RUNTIME="$(pwd)/scripts/compiler-runtime"
```

Then run the pinned eslint-plugin-svelte fixture suite:

```sh
cargo test --locked --test upstream_parity -- --report reports/upstream-parity.json
```

The runner checks exact diagnostic counts, messages and one-based UTF-16 line/column positions. It preserves duplicate findings and tests fix output in one pass, matching the upstream fixture generator. Each case runs only its target rule. Existing local regression fixtures remain in `fixtures/linter`.

The corpus is pinned to [18339c886320151148568063c5801bf69cb51027](https://github.com/sveltejs/eslint-plugin-svelte/commit/18339c886320151148568063c5801bf69cb51027), plugin 3.23.0. Every imported file has a SHA-256 hash. `.gitattributes` preserves upstream bytes on Windows. Missing, changed and extra corpus files fail the check.

The initial run contains 1,299 inputs for 83 rules using the upstream fixture-loader naming convention. Upstream has 84 top-level fixture directories; `no-conflicting-module-names` uses custom tests and ordinary `Foo.svelte` filenames instead of that loader. Its files are retained but its cases are not executed yet. The declared comparison environment uses JobSys's current locked Svelte 5.49.2, ESLint 10.9.1, TypeScript 6.0.3 and @typescript-eslint/parser 8.70.0. These versions control dependency eligibility; the Rust runner does not invoke ESLint. Compiler-backed cases invoke Svelte, and type-aware cases invoke the TypeScript checker through Node.js.

## Initial results

| Result | Cases |
| --- | ---: |
| All checked expectations match | 689 |
| Mismatches or unsupported capabilities | 546 |
| Outside the declared dependency versions | 64 |

| Priority rule | Match | Gap | Version skip |
| --- | ---: | ---: | ---: |
| require-each-key | 3 | 0 | 0 |
| no-navigation-without-resolve | 36 | 48 | 0 |
| no-unused-props | 56 | 20 | 0 |
| prefer-svelte-reactivity | 78 | 1 | 0 |
| prefer-writable-derived | 2 | 7 | 0 |
| valid-prop-names-in-kit-pages | 7 | 4 | 11 |

These are fixture results, not real-project finding counts. A gap can be a location or message mismatch even when both tools report the same number of findings. All seven writable-derived invalid cases match their diagnostic expectations but require suggestions, which Oxvelte does not expose. The initial reactivity gap was a default-exported binding in a `.svelte.js` module; it is now fixed and all 79 reactivity cases match. Navigation cases expose argument-location differences as well as semantic mismatches. All 56 unused-props valid cases pass. Its 20 invalid cases have diagnostic differences, mostly report locations, with one custom-options case missing a finding.

Unused props now matches all 76 cases, navigation resolve all 84, and writable derived all nine. The [seventh parallel wave report](research/upstream-parity-seventh-wave.md) records the latest changes: all seven type-aware condition cases match, bringing the full suite to 1,232 matches, zero gaps and 67 version skips. The current full-suite counts and remaining issues are in the generated gap inventory below.

The JSON report contains every case ID, status and expected/actual difference. It separates count, diagnostic, fix, parse, version and capability issues. It records unsupported rules, executable configs, compiler-dependent rules, extra parser settings and suggestions. All rule options are passed through. Typed cases run and can fail; they are not skipped wholesale. Unsupported capabilities prevent a case being counted as a full match even when its diagnostics agree.

## Tracking progress

The [gap inventory](research/upstream-parity-gaps.md) categorizes every outstanding case. The [commit plan](research/upstream-parity-plan.md) proposes separate fixes and names the deferred capabilities. Regenerate the inventory after each fix with `node scripts/summarize-parity.mjs` using a fresh full report.

```sh
# Focus on a rule. An unknown name fails instead of running zero tests.
cargo test --locked --test upstream_parity -- --rule no-unused-props --report reports/props-parity.json

# Require every eligible case to match, ignoring the known-failure allowance.
cargo test --locked --test upstream_parity -- --rule require-each-key --strict

# Review the full report before explicitly replacing the known-gap baseline.
cargo test --locked --test upstream_parity -- --update-baseline --report reports/upstream-parity.json
```

`tests/upstream-parity-baseline.json` stores issue signatures by case and comparison dimension. CI rejects new or changed gaps and changed version skips. Removed issues are improvements and pass. Updates require a full suite run and are tied to the manifest hash, including its environment. Updating the baseline does not edit upstream expectations. CI runs the adapter tests and parity runner on Windows and Linux and uploads the full report.

The initial baseline was inspected for the priority rules and grouped by issue category. It is a record of current compatibility debt, not a claim that 546 gaps are acceptable final behavior. Fix rules against the unchanged expectations and reduce the baseline as improvements land.

## Reproducing the import

The importer has separate Node dependencies. Regular parity runs require Cargo, Node.js and the pinned compiler runtime above. CI installs both dependency sets and sets the runtime path on Windows and Linux.

```sh
git clone https://github.com/sveltejs/eslint-plugin-svelte /path/to/upstream
npm ci --prefix scripts/upstream-parity --ignore-scripts
node scripts/upstream-parity/import.mjs /path/to/upstream --check
```

The importer reads the pinned Git blobs, including support files, rule sources, test sources and the MIT license. Local modifications and Git line-ending conversion cannot alter the import. It reproduces the upstream loader's filename discovery, JSON/JS/CJS config precedence and dependency filters. It exports original YAML diagnostics and fix outputs to `manifest.json`, preserving suggestions and parser/config metadata. JS/CJS configs are retained unchanged; eligible compiler configuration modules are executed by the compiler bridge. Missing error/fix snapshots fail the import; it never generates expectations by running Oxvelte.

Metadata inspection uses the exact declared ESLint and TypeScript versions to resolve inherited core fixability and statically proven compiler-version guards. In particular, const-tag migration requires Svelte 5.56 and is skipped under the declared 5.49.2 environment. Diagnostic filename spelling is normalized only for the evaluated fixture when comparing messages; actual file paths remain available to project resolution. See the fourth-wave report for the reviewed metadata and baseline identity changes.

For a future upstream refresh, change the pinned revision and version in the importer and runner together, import into a fresh reviewed corpus directory, inspect expectation changes, then generate a full report and review the baseline. The importer refuses to overwrite modified files or leave stale files behind. The generated `tests/package.json` binds Oxvelte's nearest-package Svelte-version signal to the declared environment, while upstream's original package metadata is preserved unchanged.

## Remaining coverage

This first runner covers the raw rule fixtures. The imported TypeScript/core integration test sources, processor/config/settings tests and custom `no-conflicting-module-names` tests are available for later ports but are not executed. It also does not run a reference ESLint installation to validate each snapshot under the declared environment. Expectations come from the pinned upstream files. All eligible raw rule fixtures now match. Type-aware condition checks have scope limitations documented in the seventh-wave report. The existing real-project parity script remains useful alongside these checks.

See the [research note](research/eslint-plugin-svelte-test-suite.md) for the upstream test architecture and the JobSys motivation. The upstream MIT license is copied into the corpus and its attribution is retained in `THIRD_PARTY_NOTICES`.

After commits 1-8, the suite has 762 matches, 473 gaps and 64 version skips. Unused props matches all 76 cases, navigation-without-resolve matches all 84, and Kit prop names matches all 11 eligible cases with 11 legacy version skips. See the [progress report](research/upstream-parity-progress.md) and [current gap inventory](research/upstream-parity-gaps.md).

The [first parallel wave](research/upstream-parity-first-wave.md) brings the suite to 789 matches, 446 gaps and 64 version skips. Suggestions are now compared as exact per-diagnostic alternatives. Duplicate directive and navigation-base fixtures all match.
