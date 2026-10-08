# Upstream parity version matrix

All 67 cases skipped by the primary environment now pass in at least one matching version profile. Across the primary environment and four profiles, all 1,299 imported cases pass, with zero eligible gaps in each environment. The corpus remains pinned to eslint-plugin-svelte 3.23.0 at revision `18339c886320151148568063c5801bf69cb51027`.

The primary result remains 1,232 passes, zero gaps and 67 version skips. Those skips describe cases that do not apply to its pinned dependencies. They are now tested independently, without changing the primary corpus or baseline.

## Results

| Profile | Svelte | ESLint | TypeScript | TS parser | Pass | Gaps | Version skips |
| --- | --- | --- | --- | --- | ---: | ---: | ---: |
| Primary | 5.49.2 | 10.9.1 | 6.0.3 | 8.70.0 | 1,232 | 0 | 67 |
| modern | 5.56.0 | 10.9.1 | 6.0.3 | 8.70.0 | 1,238 | 0 | 61 |
| svelte4 | 4.2.20 | 9.39.1 | 5.9.3 | 8.70.0 | 1,013 | 0 | 286 |
| legacyparser | 4.2.20 | 8.57.1 | 5.2.2 | 6.10.0 | 1,013 | 0 | 286 |
| svelte3 | 3.59.2 | 8.57.1 | 5.2.2 | 6.10.0 | 1,006 | 0 | 293 |

All profiles use svelte-eslint-parser 1.8.1. Older profiles skip more fixtures because newer syntax and rule behavior do not apply to them. These counts are eligibility differences, not regressions. The [coverage JSON](upstream-parity-version-matrix.json) records every original skip and its outcome in all four profiles.

The original skips comprised 54 older-Svelte cases, six newer-Svelte cases, three older TypeScript parser cases and four ESLint 8/9 cases. The older profiles passed without rule changes. The modern profile exposed three gaps involving mutable `{let ...}` declarations. Native parsing now distinguishes const and let declaration tags, preserves their source ranges and serializes let tags with the correct declaration kind. All six newly eligible modern cases now pass.

## Reproduction

Install the importer dependencies, then run the matrix from the repository root:

```sh
npm ci --prefix scripts/upstream-parity --ignore-scripts --no-audit --no-fund
node scripts/version-matrix/run.mjs
```

The runner installs each profile's locked dependencies, builds a separate corpus under `reports/version-matrix/<profile>/corpus`, and runs it with `--strict --no-baseline`. Each child process receives its own compiler runtime. Reports are written to `reports/version-matrix/<profile>/parity.json` and the combined `reports/version-matrix/summary.json`.

```sh
# Run one profile.
node scripts/version-matrix/run.mjs --profile modern

# Reuse installed profile dependencies.
node scripts/version-matrix/run.mjs --no-install

# Inspect a generated corpus without running Cargo.
node scripts/version-matrix/run.mjs --profile modern --prepare-only --no-install
```

Eligibility and inherited ESLint metadata are recomputed from the installed profile versions. Original source hashes are checked before preparing a profile. The only changed corpus support file is the generated `tests/package.json`, which supplies the profile's Svelte version to native rules. Expected diagnostics, suggestions, fixes and fixture configs remain unchanged. A custom corpus cannot implicitly read or overwrite the primary baseline, and compiler-dependent cases verify the actual runtime Svelte version against the manifest.

CI runs all four profiles on Windows and Linux and uploads their reports. The local Windows runs pass; the newly added CI jobs have not yet run remotely.

## Validation and limits

All four strict profile runs and a fresh strict primary run pass. The full Cargo suite passes, including 586 library tests, one CLI test, nine parser contract tests and 13 adapter tests. The six importer tests, four categorizer tests and three matrix runner tests pass. Formatting and diff checks pass.

The corpus audit confirmed identical original case IDs, configs, requirements and expectations in every generated profile. All original source files are byte-identical except the generated package metadata. The union of passing cases covers all 1,299 original IDs, and all 67 original skips are covered with no gaps. The primary corpus and baseline have no changes.

These runs compare Oxvelte against frozen upstream snapshots. They do not execute the reference ESLint plugin's complete test harness. Installed dependency versions determine eligibility and metadata; compiler and type-aware rules invoke the actual Svelte compiler and TypeScript checker. Passing this corpus does not establish parity for arbitrary projects or the unported integration, processor, settings and custom tests described in [the runner documentation](../upstream-parity.md#remaining-coverage).

Type checker projection limits from the [seventh-wave report](upstream-parity-seventh-wave.md) still apply, including local template scopes following const or let declarations. JobSys has not been rerun as part of this matrix work.
