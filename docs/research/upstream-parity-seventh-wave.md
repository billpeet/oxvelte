# Seventh parallel wave: type-aware conditions

All **seven remaining eligible cases** now match the pinned upstream expectations. Three workers implemented the TypeScript service, expression checks and configuration adapter in isolated worktrees based on `9a30a68`; the coordinator integrated the Rust rule and shared runtime, reviewed the source projections and completed the baseline audit.

| Result | Before wave 7 | After wave 7 |
| --- | ---: | ---: |
| Matches | 1,225 | 1,232 |
| Gaps | 7 | 0 |
| Version skips | 67 | 67 |
| Total cases | 1,299 | 1,299 |

The cases all target `@typescript-eslint/no-unnecessary-condition`. Five invalid fixtures expect eight diagnostics; two valid fixtures guard reactive/template behavior. The optional-chaining fixture also requires exact automatic fix output. All seven pass strict comparison.

## Changes

An opt-in rule obtains flow-sensitive types from a genuine TypeScript program through the existing Node runtime. It resolves the project's TypeScript package and nearest tsconfig, accepts an explicit project path, supports an unsaved component overlay and resolves project imports and compiler paths. Without a tsconfig it uses strict defaults. Source masking preserves original UTF-16 offsets and line endings; template queries have explicit projections back to their original ranges. Syntax errors are rejected before recovered types can produce misleading findings.

Svelte reactive exclusions use checker symbol identity for mutable component-root variables. Ordinary script expressions retain TypeScript narrowing, while `$:` and template expressions referencing those mutable bindings remain conditional. Locals inside reactive blocks remain eligible, including locals that shadow a root name. Immutable roots are checked. The handlers cover truthiness and unary negation, literal comparisons, nullish operands, optional chaining and fixes, conditional expressions, loops, array predicate callbacks, uncertain types and array-index exceptions.

The parity adapter now forwards the upstream suite-level strict project that the manifest did not include in individual case configs. It uses the unchanged imported tsconfig; unknown parser settings remain explicit gaps. The runtime bootstrap travels over stdin, avoiding Windows command-line length limits as the embedded service grows. Type-check requests refresh their program rather than reusing stale types from edited source or imports. CI runs the new checker regressions on Windows and Linux.

| Part | Integrated commits |
| --- | --- |
| Genuine TypeScript sessions and projections | `5776044`, `e517bc4` |
| Condition handlers and Svelte regressions | `e430d11`, `e7fd50c` |
| Suite project forwarding | `bdd53dc` |
| Native rule and runtime integration | `b9ff98c` |
| CI checker tests | `45b105f` |

The implementation follows the [pinned Svelte extension rule](https://github.com/sveltejs/eslint-plugin-svelte/blob/18339c886320151148568063c5801bf69cb51027/packages/eslint-plugin-svelte/src/rules/@typescript-eslint/no-unnecessary-condition.ts), preserved in the corpus. Its metadata marks this extension deprecated because newer svelte-eslint-parser versions make the extension unnecessary. The frozen cases remain part of our parity contract.

## Validation and audit

The full Cargo suite passes: 585 library tests, one CLI test, seven parser-contract tests, twelve adapter tests and the full parity check. All 31 runtime tests pass, including eight service tests and ten condition-handler tests; the two executable compiler-configuration tests also pass. Formatting and diff checks pass.

The audit against `9a30a68` confirms exactly seven removed gap cases. Every other case, retained issue hash and all 67 version skips remain unchanged. The declared environment, manifest and all 2,824 corpus files are unchanged. The reviewed baseline now contains no eligible gap cases. This establishes exact agreement for all 1,232 eligible fixtures, not every possible Svelte program.

## Configuration and limits

This rule is non-recommended. It runs when all rules are selected and can be disabled through configuration. It requires Node.js, TypeScript installed relative to the component, an actual `.svelte` filename and a TypeScript script. `settings.typescript.project` accepts one tsconfig path, resolved relative to the component directory; arrays and glob patterns produce an actionable error. The pinned parity runtime is selected through `OXVELTE_COMPILER_RUNTIME` as described in the [setup instructions](../upstream-parity.md).

Template expressions with script bindings are projected. Each bodies, snippet bodies, await branches with local bindings and expressions following template const or let declarations are conservatively omitted until their local scopes can be modeled. Branch-dependent template narrowing is not modeled. Instance and module scripts currently share one virtual TypeScript module, so same-name declarations in those separate script contexts remain a limitation. The checker creates a fresh program per component; project-wide program reuse and performance tuning remain future work.

All 67 version skips are still outside the declared pinned environment. Imported integration/processor/configuration suites beyond the raw rule fixtures remain unported. The JobSys comparison has not been rerun.
