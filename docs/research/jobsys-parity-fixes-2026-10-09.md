# JobSys parity fixes, 9 October 2026

The real rule gaps from the [initial JobSys check](jobsys-parity-2026-10-09.md) are fixed. On the 989 files both tools can parse, all 1,029 Oxvelte diagnostics now match ESLint exactly on file, rule, full source range, message and severity. There are zero Oxvelte-only diagnostics and no missing distinct ESLint findings.

ESLint reports 1,030 entries because it emits the same `new Set<string>()` warning twice in `url-params.svelte.ts`. Oxvelte reports that constructor once. The initial report incorrectly described the unmatched duplicate as a missed mutable set; this was a report interpretation error, not a rule implementation gap. A reduced exported-function fixture confirms the single native diagnostic and its source span. The original count comparison remains recorded, with this correction attached.

ESLint still cannot parse `AttachmentGallery.svelte`. Oxvelte evaluates it and reports four diagnostics, which are excluded from the comparable totals. Raw Oxvelte totals are therefore 1,033. Fixes and suggestions were not compared in this project run.

The [result JSON](jobsys-parity-fixes-2026-10-09.json) retains per-rule results, the reference duplicate, parser exclusion and validation counts.

## Changes and fixtures

Sixteen local fixtures under `fixtures/jobsys` cover the observed failure patterns and controls. They run through the real project-config lint APIs in `tests/jobsys_regressions.rs`, with exact diagnostic count and message assertions. The mutable-set fixture also checks the reported source text. Ten unused-prop fixtures were checked against the installed reference plugin with JobSys's actual TypeScript project service, rather than a parser-only setup that would disable the reference rule.

| Change | Cause and resulting behavior |
| --- | --- |
| Kit 3 navigation eligibility | Upstream accepts Kit 1/2 only. Oxvelte now reads the installed Kit version, with declared package metadata as a fallback, and skips unsupported majors. The Kit 2 control still reports unresolved navigation. Unknown versions retain the previous behavior. |
| Block-scoped function options | Explicit `blockScopedFunctions: "allow"` now suppresses strict-mode function declarations. The disallow control still reports them. Without resolved version-dependent defaults, legacy behavior is retained. |
| Nested style validation | HTML nested styles are accepted by Svelte. The system rule no longer rejects them. The optional `no-nested-style-tag` rule still checks the style convention. |
| Multiline prop types | OXC parses type members, so callback union continuations no longer become fictitious `\|` and `\| undefined` properties. Original type-member offsets are retained. |
| Type declaration bounds | Index signatures and type aliases stay within their own declarations. Unrelated later declarations no longer affect props, and callback arrows do not prematurely end intersection types. |
| Destructuring and rest bindings | OXC binding patterns replace text scanning. A default such as `"Select..."` is no longer mistaken for a rest binding, so unused `getId` is reported. Comments containing commas no longer hide used callbacks. |
| Array-valued props | Array element fields are not treated as properties of the array-valued prop. Named and inline element types are covered. |
| Nested object consumption | Optional access, bare object references such as aliases, and shorthand `bind:` usage consume properties consistently with upstream. Script references use resolved binding symbols. |

The frozen upstream corpus and baseline were not edited. Fixtures were committed first, followed by separate implementation commits. The regression test is included in Windows and Linux CI alongside the upstream adapter tests.

## Verification

The fresh reference run used clean JobSys revision `c48e736e90db287e405f9b11ebf549f546d2dc48`. The app changed after the initial report, so its old ESLint output was not reused for final validation. The reference source snapshots still matched the files at comparison time, and all Oxvelte batches completed without stderr or process failures. JobSys's tracked files were unchanged by this work.

Installed versions remain ESLint 10.9.1, eslint-plugin-svelte 3.23.0, Svelte 5.57.2, SvelteKit 3.0.1, TypeScript 6.0.3, typescript-eslint parser 8.70.0 and svelte-eslint-parser 1.8.1.

The full Cargo suite passes: 586 library tests, one CLI test, 16 JobSys regressions, nine parser contracts and 13 adapters. Primary upstream parity remains 1,232 passes, zero gaps and 67 version skips. All four strict version profiles pass with zero eligible gaps, covering all 67 original skips and all 1,299 imported cases across the environments. Local verification was on Windows; the updated remote CI has not run yet.

```sh
cargo test --locked --test jobsys_regressions
cargo build --release --locked
node scripts/parity-project.mjs C:/Users/marti/source/JobSys/jobsys-app
```

For a faster development comparison, `OXVELTE_BINARY` can select a debug build. `--reuse-eslint` is appropriate only while project sources, dependencies and config remain unchanged. Passing this project check does not cover arbitrary projects or the unported upstream integration tests described in [the parity documentation](../upstream-parity.md#remaining-coverage).
