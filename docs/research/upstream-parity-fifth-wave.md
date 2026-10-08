# Fifth parallel wave: indentation

Wave 5 adds opt-in `svelte/indent`, using the pinned upstream token-offset algorithm. Three workers implemented Svelte templates, JavaScript and TypeScript in isolated worktrees based on `4ed840a`; the coordinator supplied the shared engine and reviewed the combined implementation.

All **91 eligible indentation cases** match exact diagnostic counts, messages, UTF-16 locations and one-pass fix output. Five indentation cases remain version-skipped under the unchanged declared environment.

| Result | Before wave 5 | After wave 5 |
| --- | ---: | ---: |
| Matches | 1,044 | 1,135 |
| Gaps | 188 | 97 |
| Version skips | 67 | 67 |
| Total cases | 1,299 | 1,299 |

## Changes

| Part | Integrated commits |
| --- | --- |
| Shared token/offset engine and registration | `38be7b7`, `55dbaa4`, `1060fdb`, `2a086f8`, `34634df` |
| JavaScript visitor and corrections | `c49a282`, `ee3e732`, `7e6b3f6` |
| TypeScript visitor and corrections | `22afb95`, `ef53e94` |
| Svelte template visitor and corrections | `c028e11`, `5ee6957`, `d5bde7b`, `7f62921`, `b625e1d` |
| Template parser prerequisites | `b6b4112`, `7cd0cc4`, `3b37e6f`, `95a8f4a` |
| TypeScript parity parser contract | `af767d9` |

The shared engine collects source tokens and comments, maintains relative indentation and alignment offsets, caches effective indentation by line, and reports whitespace-only fixes. It preserves source byte ranges while comparing UTF-16 columns, handles mixed tabs/spaces and CRLF, and anchors scripts to their actual opening tags even when attributes contain angle brackets.

The visitors cover nested elements and attributes, template expressions, if/each/await/key blocks, snippets, const/debug/render tags, JavaScript declarations and continuations, and TypeScript types, signatures, mapped types, decorators, namespaces and import attributes. Numeric or tab indentation, `indentScript`, `switchCase` and vertical attribute alignment follow upstream defaults. Indentation remains non-recommended, matching upstream; it runs when explicitly selected or when all rules are selected.

A separate parser prerequisite accepts whitespace between `{` and control-tag sigils, preserving original spans. Spread attributes also retain their proper metadata when whitespace precedes the ellipsis. These constructs occur in upstream's multiline indentation inputs and must be parsed as their actual template nodes. Comment and regex regressions ensure the expanded tag recognition preserves JavaScript expressions. The existing spaced-tag contract test now checks structured snippet/render metadata rather than the previous three placeholder expressions (`73d36ac`).

One TypeScript fixture uses `readonly protected` modifiers. The pinned `@typescript-eslint/typescript-estree` 8.70.0 with TypeScript 6.0.3 accepts this with no parse diagnostics; OXC emits checker diagnostic TS1029 while retaining the full AST. The parity adapter now excludes **only TS1029 for TypeScript**. Tests retain malformed syntax, JavaScript parse failures and duplicate-modifier TS1030. The TS1029 adapter correction leaves production parser behavior and upstream expectations unchanged. The primary parser implementation is [ast-converter.ts](https://github.com/typescript-eslint/typescript-eslint/blob/v8.70.0/packages/typescript-estree/src/ast-converter.ts); its opt-in checker diagnostics are listed in [semantic-or-syntactic-errors.ts](https://github.com/typescript-eslint/typescript-eslint/blob/v8.70.0/packages/typescript-estree/src/semantic-or-syntactic-errors.ts).

## Validation

Strict indentation parity and the full Cargo suite pass: 577 library tests, one CLI test, seven parser-contract tests, nine adapter tests and the full parity check. The importer's five tests, categorizer's four tests, formatting and diff checks pass. The audit against `4ed840a` confirms exactly 91 removed gap cases and no other changed results. Retained issue hashes, all 67 skips, the declared environment, manifest and all 2,824 corpus files remain unchanged.

Native regressions cover Unicode/CRLF source ranges, whitespace characters, comments, script anchors, tabs and script indentation, private-in expressions, TypeScript token boundaries and idempotent fixes, multiline block metadata and template bindings.

## Remaining work and limits

| Work | Gap cases |
| --- | ---: |
| Unused Svelte ignores and compiler warnings | 46 |
| Compile validation and executable compiler configuration | 44 |
| TypeScript unnecessary-condition integration | 7 |
| Total | 97 |

Compiler-related work totals 90 cases; six need executable fixture configuration. The [current inventory](upstream-parity-gaps.md) lists the remaining cases.

Frozen-suite matches do not establish parity for every possible program or option. `ignoredNodes` currently supports JavaScript/TypeScript node-type, comma-separated, child and descendant selectors; full esquery attribute/pseudo/field/sibling selectors and template-node exclusions remain unimplemented. JSX indentation compatibility and multiline script/style attribute values are unverified. These options are not exercised by the pinned indentation fixtures. The JobSys comparison has not been rerun.
