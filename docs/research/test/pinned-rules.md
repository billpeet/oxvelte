# Proposal: pinned rule fixtures — one home, one automated update pipeline

Status: proposal, not started. This document is meant to be readable on its own,
without knowing the codebase first.

## Background: why we have two fixture collections today

Our linter is a drop-in replacement for `eslint-plugin-svelte`, so every rule we
ship must produce the same messages at the same positions as that upstream
project. To prove that, we keep upstream's example files (called fixtures) and
their expected results, and our test suite compares our output against them.

Today those fixtures live in two places:

- `fixtures/linter/` — an old, hand-copied snapshot of upstream's fixtures.
  Each rule has its own folder with `valid/` examples (the rule must stay
  silent) and `invalid/` examples (the rule must fire), plus `-errors.yaml`
  files that record the expected message, line, and column for each finding.
  These are checked by `cargo test --lib`.
- `fixtures/upstream/eslint-plugin-svelte/` — a newer, machine-generated copy.
  The script `scripts/upstream-parity/import.mjs` downloads upstream's fixtures
  at one pinned commit and converts all of their expectation files into a
  single `manifest.json`. These are checked by `cargo test --test upstream_parity`.

The second collection was built next to the first instead of replacing it, so
the two mostly duplicate each other. Nothing user-facing depends on the old
copy — these files are only test data.

One more fact that matters for everything below: **a fixture is only a test.**
A rule works because a Rust file implements it. Fixtures prove the rule works;
they do not provide the rule.

## Goals

1. Keep tracking upstream `eslint-plugin-svelte` as it evolves, by moving the
   pinned commit forward when we choose to.
2. Never silently lose rules or test cases that upstream removes but we want
   to keep.
3. Store each test case in exactly one place.
4. Make the whole update a single command, after one manual cleanup.

## The one-time cleanup

Done once, by hand, before the automated pipeline can exist.

1. **Consolidate to one fixture home.** Today the same rule's examples can
   exist under both `fixtures/linter/<rule>/` and inside the upstream import
   tree. We merge everything into a single `fixtures/` layout where each rule
   has one folder containing its `valid/` and `invalid/` examples, and both
   test commands (`cargo test --lib` and `cargo test --test upstream_parity`)
   read from that same home. This removes the duplication by construction:
   after the merge there is only one place a case can live.
2. **Dedupe.** During the merge, cases that exist in both collections with
   identical expectations are kept once. Cases that only exist in the old
   snapshot (typically ones upstream has since deleted) are kept too — they are
   exactly the material the backup role exists for.

After this step, `fixtures/linter/` no longer exists as a separate collection;
there is one shared fixture tree that both test commands use.

## The automated update pipeline

Run whenever we want to move the pinned upstream commit forward, via a single
cargo command (see the next section). The command drives one script — either
a revised `scripts/upstream-parity/import.mjs` or a new wrapper around it —
which performs all of these stages in order:

1. **Stage the new import.** The importer downloads upstream's fixtures at the
   new commit and writes a new `manifest.json` into a scratch `tmp/` directory
   that is not tracked by git. The live manifest at
   `fixtures/upstream/eslint-plugin-svelte/manifest.json` is untouched at this
   point.
2. **Diff old against new.** The script compares the staged manifest against
   the live one, case by case (every case has a stable identifier, so this is
   mechanical). The diff produces three lists:
   - **Removed rules** — an entire rule disappeared upstream. These are
     backup candidates: we may want to keep supporting a rule upstream dropped.
   - **Removed or renamed cases** — individual examples changed or vanished
     while the rule itself remains. This is normal upstream housekeeping; we
     follow upstream and do not back these up. (A deleted case does not mean a
     deleted rule — cases are removed for ordinary reasons like behavior tweaks
     or test reorganizations.)
   - **Changed expectations** — the same case now expects a different message
     or position. That means upstream intentionally changed the rule's
     behavior, and the correct response is work in our Rust rule, not a backup.
3. **Back up rules we choose to keep.** For each rule in the first list, the
   script asks (or reads a decision list) whether to retire it with upstream or
   keep it. Kept rules have their complete cases — examples, expected findings,
   per-case configuration, Svelte-version requirements, and expected fix
   output — copied into the consolidated fixture tree, so our Rust rule keeps
   its tests. The backup keeps the full case, not just the message and
   position, because the rest is what makes the case runnable.
4. **Promote the staged manifest.** The staged `manifest.json` replaces the
   live copy, and the new upstream fixture files replace the old. The scratch
   `tmp/` directory is discarded.
5. **Hand over to the test suite.** `cargo test --lib` and
   `cargo test --test upstream_parity` now run against the new pin. Any
   failures at this point are the genuinely remaining work: the "changed
   expectations" list from stage 2, meaning rules whose behavior upstream
   changed and whose Rust implementations we must update to match. Everything
   else — added cases, removed cases, backups — is already handled by the
   script.

## What this gives us

- One command moves the pin, instead of a manual import plus archaeology.
- Nothing upstream deletes can silently disappear from our support.
- Each test case exists once, in one tree both test commands share.
- After a pin move, a failing test always means "a rule's behavior changed
  upstream and our Rust code needs updating" — never "the fixture bookkeeping
  is stale."

## Running it: one cargo command for everything

Today our helper scripts are run three different ways: tests go through cargo
(`cargo test --lib`, `cargo test --test upstream_parity`), some jobs are bash
scripts, and the fixture importer is run with `node` by hand. That is a lot to
remember, and this proposal adds more `.mjs` scripts on top. To keep one
consistent front door, we orchestrate the JavaScript scripts through cargo
itself, using three pieces that already exist or are small to add:

1. **Environment variables in `.cargo/config.toml`.** This file already sets
   `OXVELTE_COMPILER_RUNTIME` for every cargo command. We add entries for the
   pinned upstream commit and for each script path the launcher can run:

   ```toml
   [env]
   ESLINT_PLUGIN_SVELTE_GIT_HASH = "github_commit_hash_here"
   ESLINT_FIXTURES_IMPORT_SCRIPT = { value = "scripts/upstream-parity/import.mjs", relative = true }
   ```

   The pin now lives in exactly one place. Moving to a newer upstream commit
   means editing this one line and nothing else.

2. **A thin Rust launcher at `scripts/launcher.rs`.** Registered in
   `Cargo.toml` as an extra binary (`[[bin]] name = "launcher"
   path = "scripts/launcher.rs"`), it does very little: parse the argument it
   was given (for example `--get-eslint-fixtures`), read the matching script
   path and the pin from the environment variables above, resolve the script
   path against the workspace root, run it with `node` (or `bun`), forward the
   pin to it as an environment variable, and mirror its exit code. It
   deliberately contains no import or diff logic — JavaScript still handles
   JavaScript dependencies; the launcher only starts the right script with the
   right settings.

3. **A cargo alias.** Also in `.cargo/config.toml`, so the whole thing reads
   like a built-in command:

   ```toml
   [alias]
   get-fixtures = "run --bin launcher -- --get-eslint-fixtures"
   ```

   (The `--` hands everything after it to the launcher rather than to cargo.)

   Running `cargo get-fixtures` then builds the tiny launcher and starts
   `scripts/upstream-parity/import.mjs` with the pinned commit forwarded — no
   `node` command to remember, no script path to type.

To make this work, `scripts/upstream-parity/import.mjs` stops hardcoding the
commit in `export const revision = '...'` and instead reads the
`ESLINT_PLUGIN_SVELTE_GIT_HASH` environment variable the launcher forwards.
As a bonus, `tests/upstream_parity.rs` can read the same variable for its
`REVISION` constant, since cargo sets these environment variables for every
command it runs — so the importer and the test suite can never disagree about
which commit is pinned.

The same pattern extends to any future script the pipeline needs (the staged
import, the manifest diff, the backup step): add one environment variable with
its path, teach the launcher one new argument, and optionally add one alias.
Everything we run becomes `cargo <something>`.


## Open questions to settle when we build this

- Whether the consolidated fixture home keeps the current
  `fixtures/linter/`-style file layout (one folder per rule with loose files),
  or adopts the manifest's packed format everywhere, with the test harnesses
  reading that instead.
- How the script asks about removed rules: interactive prompt, or a small
  committed decision file listing rules we always keep.
- How the upstream-parity gap baseline (`tests/upstream-parity-baseline.json`)
  is regenerated after a pin move, since moving the pin changes which gaps
  exist.

Settled by the launcher design above: the pinned commit lives only in
`.cargo/config.toml`, both the importer and the parity test read it from
there, and the whole pipeline is launched through cargo aliases rather than
remembered `node` or bash commands.
