# Remaining seven cases: type-aware unnecessary conditions

The remaining gaps all target `@typescript-eslint/no-unnecessary-condition`, the Svelte plugin's extension rule. They are seven unsupported fixtures, not seven diagnosed behavioral differences. Five invalid fixtures expect eight diagnostics; two valid fixtures expect none. Only optional chaining changes fix output.

## Shared prerequisite

Oxvelte currently has no TypeScript checker service. Add a reusable type-query boundary that obtains flow-sensitive types from a genuine TypeScript program, preserves original source locations and respects project compiler options, including strictNullChecks. The existing Node runtime is a possible transport; its compiler transpilation alone does not provide type information. Keep this opt-in and report unavailable type services clearly. Do not replace the checker with initializer/name heuristics or delegate the whole rule to ESLint.

Before registering the rule in parity, establish the type-query contract and test imports, shadowing, assignments, unions, unknown/any and source mapping. Package resolution and project configuration need an explicit contract. Register the exact imported rule name and retain unchanged fixture expectations.

## Work packages

| Package | Fixtures | Expected behavior | Dependencies |
| --- | --- | --- | --- |
| Svelte scope and reference handling | valid/reactive-statement01, valid/template01; reactive exclusions in all five invalid cases | No false positives for mutable component-root references inside `$:` or templates. Local variables declared inside a reactive block remain eligible. Resolve references by binding, including shadowing. | Type service and native scope contract |
| Truthiness | invalid/example, invalid/test01 | Report `foo` as always falsy in ordinary `foo || 42`; report the argument `foo` as always truthy under `!foo && bar`. Two diagnostics total; unchanged fix output. | Type service and Svelte exclusions |
| Literal comparison and nullish coalescing | invalid/binary-expression01, invalid/nullish-coalescing01 | Report ordinary and reactive-block-local `foo == null` / `bar == null` comparisons as literal comparisons; report always-nullish left operands of `??`. Four diagnostics total; unchanged fix output. | Type service and Svelte exclusions |
| Optional chaining and fixes | invalid/optional-chaining01 | Report exactly at `?.` in ordinary and reactive-block-local accesses; replace `?.` with `.` while preserving the root-variable reactive access. Two diagnostics and exact one-pass fix output. | Type service and Svelte exclusions |

The packages own different expression handlers, but share type queries and Svelte reference handling. Freeze those interfaces before dispatching isolated worktrees. After the shared prerequisite and scope handling land, the three expression packages can run in parallel.

## Suggested commits

1. TypeScript program/type-query service, configuration and source mapping.
2. Rule registration and Svelte scope/reference exclusions, with the two valid fixtures and independent negative regressions.
3. Truthiness and unary negation.
4. Literal comparisons and nullish coalescing.
5. Optional-chain diagnostics and safe fixes.
6. Full-suite audit, reviewed baseline reduction and report.

The baseline should only lose cases once every expected diagnostic, location and fix matches. Registration alone may expose additional differences in these previously unsupported cases; inspect them rather than accepting new baseline signatures.

## Scope

These fixtures cover a small part of the upstream rule. The implementation also contains loops, conditional expressions, array predicate callbacks, any/unknown/never handling, array-index exceptions, nullable properties and two rule options. Decide and document support for those before describing the implementation as full rule parity. Its upstream metadata marks this extension deprecated because newer svelte-eslint-parser versions make the extension unnecessary; that does not remove its seven cases from our frozen suite.

The source and original tests are available in the unchanged corpus under `fixtures/upstream/eslint-plugin-svelte/src/rules/@typescript-eslint/no-unnecessary-condition.ts` and `tests/src/rules/@typescript-eslint/`. This plan comes from inspecting those sources, all seven inputs, their diagnostic snapshots and fix outputs. No implementation or baseline changes accompany this breakdown.
