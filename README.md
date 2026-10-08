<p align="center">
  <img src="assets/oxvelte.svg" alt="oxvelte logo" width="200">
</p>

# oxvelte

> [!NOTE]
> **This is a fork of [tolgaouz/oxvelte](https://github.com/tolgaouz/oxvelte).** oxvelte was created by [@tolgaouz](https://github.com/tolgaouz), and all credit for the original design and implementation goes to him. The original repository looks to be no longer actively developed, so this fork ([billpeet/oxvelte](https://github.com/billpeet/oxvelte)) continues the work. It is not affiliated with or endorsed by the original author.

A Svelte linter written in Rust. Drop-in replacement for [eslint-plugin-svelte](https://github.com/sveltejs/eslint-plugin-svelte) — same rules, same diagnostics, **50-1000x faster**.

<p align="center">
  <img src="assets/compare.gif" alt="Side-by-side benchmark linting shadcn-svelte (1,603 files): eslint-plugin-svelte takes ~15s while oxvelte completes the same lint hundreds of times in the same window" width="900">
</p>

> **This codebase was written mainly with LLM assistance.**

## Using with oxlint (recommended setup)

The fastest way to lint a SvelteKit project is **oxlint + oxvelte** together. They handle different concerns with zero overlap:

| Tool | What it lints | File types |
|------|--------------|------------|
| [oxlint](https://oxc.rs/docs/guide/usage/linter) | General JS/TS rules (`no-unused-vars`, `no-console`, type checks, imports, etc.) | `.js`, `.ts`, `.svelte` (`<script>` blocks only) |
| **oxvelte** | Svelte-specific rules (`infinite-reactive-loop`, `no-at-html-tags`, template issues, etc.) | `.svelte` |

oxlint can lint the JavaScript/TypeScript inside `.svelte` `<script>` blocks, but it does not lint Svelte templates, styles, or Svelte-specific component semantics. oxvelte covers that Svelte-specific layer: reactive patterns, template structure, style-related checks, and component conventions.

### Quick setup

```bash
# Install both
npm install -D oxlint @billpeet/oxvelte

# Add to package.json
```

```json
{
  "scripts": {
    "lint": "oxlint && oxvelte lint src/"
  }
}
```

That's it. Both tools work out of the box with zero config and sensible defaults.

Compiler diagnostics and unused `svelte-ignore` checks use Node.js and the project's installed Svelte compiler. `svelte/valid-compile` is opt-in; the recommended unused-ignore rule starts the compiler when it needs to check warning codes. TypeScript, Babel and stylesheet transformations use the project's corresponding packages. The compiler process and results are reused during a lint run. See [compiler setup and parity testing](docs/upstream-parity.md) for the pinned test runtime.

The opt-in `@typescript-eslint/no-unnecessary-condition` rule uses the project's TypeScript checker to report redundant conditions and optional chains while respecting Svelte reactive variables. It runs with `--all`; `settings.typescript.project` can select one tsconfig path relative to the component. See the [seventh-wave report](docs/research/upstream-parity-seventh-wave.md) for template scope limits and current test coverage.

### Agent-assisted migration

This repo also includes a `migrate-to-oxvelte` skill for the [skills](https://github.com/vercel-labs/skills) CLI. It guides agents through migrating from ESLint to the default `oxlint + oxvelte` stack, including custom-rule migration where possible.

Install the skill from this repository:

```bash
npx skills add billpeet/oxvelte --skill migrate-to-oxvelte
```

For Codex:

```bash
npx skills add billpeet/oxvelte --skill migrate-to-oxvelte -a codex -g
```

To inspect available skills before installing:

```bash
npx skills add billpeet/oxvelte --list
```

Once the public skills registry indexes it, it can also be discovered with:

```bash
npx skills find oxvelte
```

### Replacing eslint-plugin-svelte + ESLint

If you're migrating from the ESLint setup (`eslint-plugin-svelte` + `@eslint/js` + `typescript-eslint`), here's how the tools map:

| ESLint stack | Replacement |
|-------------|-------------|
| `@eslint/js` (core JS rules) | `oxlint` |
| `typescript-eslint` | `oxlint --tsconfig` |
| `eslint-plugin-svelte` | **`oxvelte`** |
| `eslint-plugin-import` | `oxlint` (built-in `--import-plugin`) |

```bash
# Before (ESLint, ~3-10 seconds)
eslint src/

# After (oxlint + oxvelte, ~200-400ms)
oxlint && oxvelte lint src/
```

If you have an existing eslint-plugin-svelte config, oxvelte can convert it:

```bash
oxvelte migrate eslint.config.js --write
```

### Full SvelteKit example

For a typical SvelteKit project with TypeScript:

```json
{
  "scripts": {
    "lint": "oxlint --tsconfig tsconfig.json && oxvelte lint src/",
    "lint:fix": "oxlint --fix --tsconfig tsconfig.json && oxvelte lint --fix src/"
  },
  "devDependencies": {
    "oxlint": "^1.58.0"
  }
}
```

```bash
# optional: configure oxlint
# .oxlintrc.json
{
  "plugins": ["typescript", "import", "unicorn"],
  "rules": {
    "no-console": "warn"
  }
}

# optional: configure oxvelte
# oxvelte.config.json
{
  "rules": {
    "svelte/no-at-html-tags": "error"
  },
  "settings": {
    "svelte": {
      "kit": {
        "files": { "routes": "src/routes" }
      }
    }
  }
}
```

### CI integration

Both tools return non-zero exit codes on errors, so they work naturally in CI:

```yaml
# GitHub Actions
- run: npx oxlint --tsconfig tsconfig.json
- run: npx oxvelte lint src/
```

For JSON output (useful for custom reporters or IDE integration):

```bash
oxlint --format json
oxvelte lint --json src/
```

## Install

### From npm

```bash
npm install -D @billpeet/oxvelte    # or: pnpm add -D / yarn add -D / bun add -d
```

This installs a prebuilt native binary for your platform and puts the `oxvelte` command in `node_modules/.bin`, so it works in `package.json` scripts and through `npx oxvelte`. No Rust toolchain is needed.

Prebuilt binaries cover macOS (arm64, x64), Linux (x64 and arm64, glibc and musl) and Windows (x64, arm64). They are built with the `custom-rules` feature enabled.

### From GitHub

```bash
cargo install --git https://github.com/billpeet/oxvelte.git
```

### From source

```bash
git clone https://github.com/billpeet/oxvelte.git
cd oxvelte
cargo build --release
```

The binary will be at `./target/release/oxvelte`.

## Usage

```bash
oxvelte lint src/              # lint with recommended rules
oxvelte lint --json src/       # JSON output
oxvelte lint --fix src/        # auto-fix where supported
oxvelte lint --all-rules src/  # run all 78 rules
oxvelte rules                  # list available rules
```

## Configuration

Create `oxvelte.config.json` in your project root:

```json
{
  "rules": {
    "svelte/no-at-html-tags": "error",
    "svelte/button-has-type": ["warn", { "button": false }],
    "svelte/no-inline-styles": "off"
  },
  "settings": {
    "svelte": {
      "kit": {
        "files": { "routes": "src/routes" }
      }
    }
  }
}
```

**Rules** use the same names and options as eslint-plugin-svelte. Severity can be `"off"`, `"warn"`, or `"error"` (or `0`, `1`, `2`). Options are passed as the second element of a tuple: `["error", { ...options }]`.

**Settings** configure framework-specific behavior. The `svelte.kit.files.routes` setting tells SvelteKit-aware rules where your route files live.

Without a config file, oxvelte runs the **recommended** ruleset (same as eslint-plugin-svelte's `flat/recommended`).

## Custom rules

Project-specific conventions that don't belong in the shared ruleset — forbidden imports, naming schemes, required attributes — can be written in JavaScript and loaded via `customRules`:

```json
{
  "rules": { "custom/no-div-without-class": "error" },
  "customRules": ["./rules/*.js"]
}
```

```javascript
// ./rules/no-div-without-class.js
export default {
  name: "custom/no-div-without-class",
  run(ctx) {
    ctx.walk((node) => {
      if (node.type === "Element" && node.name === "div") {
        const hasClass = node.attributes.some(
          (a) => a.type === "NormalAttribute" && a.name === "class",
        );
        if (!hasClass) ctx.diagnostic("div must have a class attribute", node.span);
      }
    });
  },
};
```

Rules run in an embedded [Boa](https://boajs.dev/) engine — no Node.js dependency, no IPC. The npm package includes this; when building from source it requires the `custom-rules` feature (`cargo install … --features custom-rules`).

Full reference — AST shape, `ctx` API, auto-fix, limitations — in [`docs/custom-rules.md`](docs/custom-rules.md).

## What's implemented

Upstream compatibility is tracked separately with a pinned fixture suite and exact expectations. See [upstream parity](docs/upstream-parity.md) for current gaps and how to run it.

- **78 lint rules** from eslint-plugin-svelte, all ported to Rust
- **Full Svelte 4 + Svelte 5** template parser (106/106 parser fixture tests)
- **281 tests passing** (lint rules + parser fixtures)
- **Parallel file processing** via rayon
- **eslint-disable** / **svelte-ignore** comment directives
- **Auto-fix** support for fixable rules (`--fix`)

### Intentionally excluded rules

A few eslint-plugin-svelte rules are **not** implemented by design:

- **`valid-compile`** — this rule *is* the Svelte compiler. Running it in a linter means invoking the full compiler on every file, which defeats the purpose of a fast native tool. Svelte already reports these errors at build time.
- **`no-unused-svelte-ignore`** — requires the Svelte compiler to know which diagnostics were actually suppressed. Again, Svelte itself warns about this at build time.
- **`indent`** — a formatting rule, not a lint rule. eslint-plugin-svelte itself marks it `recommended: false` with `conflictWithPrettier: true`. oxc tracks ESLint's `indent` as [🚫 *Not intending to implement*](https://github.com/oxc-project/oxc/issues/479) (*"Deprecated stylistic rule, can be used via the stylistic eslint plugin as a JS Plugin if necessary"*), and `@typescript-eslint/indent` is [likewise deprecated upstream](https://github.com/oxc-project/oxc/issues/503). Layout belongs in a formatter — use Prettier or `oxfmt`; don't re-encode it as lint diagnostics.

These rules add latency with zero incremental value — your build step (or formatter) already catches them.

## Project structure

```
src/                    all Rust code
  main.rs               CLI entry point
  parser/               Svelte template parser
  linter/rules/         lint rules (one file per rule)
  ast.rs                Svelte AST types
Cargo.toml              dependencies
README.md               you are here
```

## License

[MIT](LICENSE). Original work copyright © [@tolgaouz](https://github.com/tolgaouz).
