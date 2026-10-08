# JobSys regression fixtures

These locally authored reductions cover gaps discovered while comparing JobSys with eslint-plugin-svelte 3.23.0. They are separate from the unchanged imported upstream corpus.

Run `cargo test --locked --test jobsys_regressions`. Each case in `cases.json` supplies its target rule, options and exact expected messages. Other rules are disabled, and positive controls ensure the fixes still detect genuine unused props, unresolved Kit 2 navigation and disallowed nested functions.

Unused-prop expectations were checked using the reference plugin with JobSys's TypeScript project service. A parser-only reference run would skip that type-aware rule and cannot validate those expectations. See the [fix report](../../docs/research/jobsys-parity-fixes-2026-10-09.md) for causes, verification and the duplicate reference reactivity finding.
