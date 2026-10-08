# PR runtime fixes

The PR review found two production issues hidden by the pinned fixture runtime. Both are corrected without changing fixture expectations or the baseline.

## Source-map dependency

The binary now embeds the MIT-licensed @jridgewell/sourcemap-codec 1.5.5 implementation. Compiler transforms use this decoder directly rather than resolving it relative to a consumer component. Standalone bridge tests load the same vendored file. Its license is preserved in the vendor directory and THIRD_PARTY_NOTICES.

A regression creates a consumer project containing only Svelte and TypeScript, verifies that the helper package cannot be resolved there, clears the parity runtime override and checks successful compilation and exact warning mapping.

## Cache freshness

Cross-request compiler result caching is removed. The shared Node process remains alive; the native lint context still shares one compile result between compiler-backed rules within that lint run. A repeated lint request therefore recompiles and rereads imported styles.

Executable compiler, Babel and PostCSS configuration graphs are tracked by file-content hashes, including imported CommonJS helpers and dependencies loaded lazily by callbacks. If any tracked file changes, Node's module cache and tracked graphs are cleared so compiler/preprocessor libraries also discard their internal configuration caches. Unchanged configurations retain stateful callback behavior. Content hashes detect edits even when file size and timestamps are unchanged.

Sass compilation now receives the component file URL, allowing relative imports to resolve from the component directory. Tests verify that editing an imported stylesheet changes diagnostics for identical component source, filename and settings in the same process.

Freshness tracking covers the supported CommonJS configuration graphs. Arbitrary ESM-loader caches and filesystem reads performed privately by user hooks are outside that graph; the component is still recompiled on each lint request. A lint context represents one run, rather than a mutable editor session.

## Validation

Regression tests cover isolated dependency resolution, direct and transitive compiler-config edits, same-size/timestamp-preserving callback edits, unchanged stateful hooks, imported Sass changes and Babel/PostCSS helper edits. The full suite retains 1,232 matching eligible fixtures, zero gaps and 67 unchanged version skips. Runtime process reuse remains supported; compilation results are no longer reused across separate runs.
