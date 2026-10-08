# Sixth parallel wave: compiler and configuration

All **90 eligible compiler/configuration cases** now match the pinned upstream expectations. Three workers implemented the compiler bridge, ignore handling and configuration adapter in isolated worktrees based on `899d2db`; the coordinator integrated the changes, corrected rule ordering and completed the full-suite audit.

| Result | Before wave 6 | After wave 6 |
| --- | ---: | ---: |
| Matches | 1,135 | 1,225 |
| Gaps | 97 | 7 |
| Version skips | 67 | 67 |
| Total cases | 1,299 | 1,299 |

| Rule | Matches | Remaining gaps | Version skips |
| --- | ---: | ---: | ---: |
| no-unused-svelte-ignore | 46 | 0 | 9 |
| valid-compile | 44 | 0 | 22 |

## Changes

Compiler diagnostics use the genuine Svelte compiler through a reusable Node.js process. Raw compilation results are cached and shared between the two rules. TypeScript, Babel function-bind syntax and supported style preprocessors are transformed with source maps; diagnostics are mapped back to original UTF-16 locations, including Unicode and CR/CRLF line endings. Unavailable style preprocessors follow upstream's stripping behavior. Compiler failures produce actionable diagnostics.

Native ignore processing reads template and script comments, tracks individual warning codes and aliases, and associates comments with template ancestors. It preserves snippet boundaries and reports missing or unused codes at their exact locations. The Svelte 4 reactive-component exception uses the resolved compiler version. Existing native rule ignores remain handled by the native rule driver.

The adapter executes all six eligible, unchanged CommonJS configuration fixtures. Warning filters and onwarn callbacks run only for warnings retained after native ignore processing and valid-compile filters. Callbacks receive original positions and compiler metadata; custom reports and warningFilter mutations are preserved. Callback operations remain uncached so stateful hooks execute each time. Configuration logs cannot corrupt the bridge's JSON response channel.

The older local fixture harness now forwards compiler/parser settings, loads executable configurations, normalizes Windows paths for Node and respects declared compiler-major requirements. CI installs the pinned runtime on Windows and Linux before fixture checks.

Implementation follows the pinned upstream [compiler warning pipeline](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/shared/svelte-compile-warns/index.ts), its ignore processing and transform helpers. Initial integration commits are `4f2fa4c` (compiler bridge), `1413fe7` (unused ignores), `8018531` / `cc42e2d` (configuration adapter), and `5471b9c` (compile rule). Callback ordering and reporting corrections finish in `07580f4`, `e27f3e7` and `b273fef`.

## Validation

The full Cargo suite passes: 584 library tests, one CLI test, seven parser-contract tests, ten adapter tests and the full parity check. All 13 runtime tests and two executable-configuration tests pass. The importer's five tests and categorizer's four tests pass, as do formatting and diff checks.

The audit against `899d2db` confirms exactly 90 removed gap cases. All other case results, retained baseline issue hashes, all 67 version skips, declared environment, manifest and all 2,824 corpus files remain unchanged. The baseline and generated inventory reflect the reviewed full report.

## Runtime requirements and remaining work

Compiler rules require Node.js, an actual `.svelte` component filename and an installed Svelte compiler. Production resolves packages relative to the component. TypeScript, Babel and style transformations use corresponding project packages. Reproducible parity runs explicitly select the pinned runtime through `OXVELTE_COMPILER_RUNTIME`; see the [setup instructions](../upstream-parity.md). Valid-compile remains opt-in. The recommended unused-ignore rule starts the compiler when it needs to check warning codes.

Executable configuration is passed through compiler settings; this work does not add automatic discovery of every project svelte.config.js. Passing the frozen suite does not establish support for arbitrary preprocessors or future compiler versions. The JobSys comparison has not been rerun.

The only remaining fixture gaps are **seven @typescript-eslint/no-unnecessary-condition integration cases**. See the [current inventory](upstream-parity-gaps.md).

The [PR runtime fixes](upstream-parity-runtime-fixes.md) supersede the cross-component result cache described above and embed the source-map decoder so consumer projects need no extra helper dependency.
