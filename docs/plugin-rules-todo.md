# Plugin Rules — Blocked on Infrastructure

Audit: 2026-04-24. **30 rules** truly blocked.

---

## Summary

| Infrastructure | Blocked | Delegated (tsgolint/oxlint) | Native (comply) | Notes |
|----------------|---------|----------------------------|-----------------|-------|
| Type checker (typescript-eslint) | 8 | 45 via tsgolint | 9 native | 8 rules not yet in oxlint |
| Module resolution (import + n) | 13 | 1 (no-mutable-exports) | 15 native | |
| Scope analysis (react + playwright) | 9 | — | 11 native | Partially unblockable via oxc_semantic |
| Full regex parser (regexp) | 4 | — | 3 native (heuristic) | Needs regex AST parser |
| Unicorn infra | 6 | 1 (filename-case) | 1 native | Mixed needs |
| **Total blocked** | **30** | | | |

The 45 typescript-eslint rules delegated to `oxlint --type-aware` (tsgolint/typescript-go)
are **already functional** when `oxlint` + `oxlint-tsgolint` are installed. They are NOT blocked.

---

## Type checker — 8 truly blocked

These rules need type-aware analysis but are NOT yet supported by oxlint's typescript rule set.

| Rule | Why |
|------|-----|
| `naming-convention` | Apply naming rules based on AST node type (interface, enum, etc.) |
| `no-unnecessary-qualifier` | Redundant namespace qualifier |
| `no-unnecessary-type-arguments` | Type arg identical to default |
| `no-useless-default-assignment` | Default param redundant with type |
| `non-nullable-type-assertion-style` | Prefer `!` over `as NonNullable<T>` |
| `prefer-readonly` | Private property never reassigned → `readonly` |
| `prefer-readonly-parameter-types` | Unmutated params → readonly type |
| `prefer-reduce-type-parameter` | Explicit type param for `.reduce<T>()` |

### Already delegated to tsgolint (45 rules — NOT blocked)

await-thenable, consistent-return, dot-notation, no-base-to-string,
no-confusing-void-expression, no-deprecated, no-duplicate-type-constituents,
no-floating-promises, no-for-in-array, no-implied-eval, no-meaningless-void-operator,
no-misused-promises, no-misused-spread, no-mixed-enums, no-redundant-type-constituents,
no-unnecessary-boolean-literal-compare, no-unnecessary-condition,
no-unnecessary-template-expression, no-unnecessary-type-assertion,
no-unnecessary-type-conversion, no-unnecessary-type-parameters, no-unsafe-argument,
no-unsafe-assignment, no-unsafe-call, no-unsafe-enum-comparison, no-unsafe-member-access,
no-unsafe-return, no-unsafe-unary-minus, only-throw-error, prefer-find,
prefer-nullish-coalescing, prefer-optional-chain, prefer-return-this-type,
promise-function-async, related-getter-setter-pairs, require-array-sort-compare,
require-await, restrict-plus-operands, restrict-template-expressions, return-await,
strict-boolean-expressions, strict-void-return, switch-exhaustiveness-check,
unbound-method, use-unknown-in-catch-callback-variable.

See `src/rules/delegated/tsgolint.rs` for full definitions.

---

## Module resolution — 13 rules

Need deeper module resolution than current ImportIndex (re-export tracking, publish analysis, require support).

| Rule | Plugin | Why |
|------|--------|-----|
| `no-named-as-default-member` | import | Check named export member on default import |
| `no-unused-modules` | import | Cross-project dead module detection |
| `no-deprecated` | import | Detect deprecated exports via JSDoc/metadata |
| `no-extraneous-dependencies` | import | Full dep validation (not just devDeps) |
| `order` | import | Enforce import ordering with group logic |
| `no-missing-import` | n | Validate import target exists (bare specifiers) |
| `no-missing-require` | n | Same for `require()` calls |
| `no-extraneous-require` | n | Extraneous deps via `require()` |
| `no-unpublished-bin` | n | Bin script points to unpublished file |
| `no-unpublished-import` | n | Import from unpublished package file |
| `no-unpublished-require` | n | Same for `require()` |
| `prefer-global/buffer` | n | Prefer `Buffer` global over `require('buffer')` |
| `prefer-global/console` | n | Prefer `console` global over `require('console')` |

**Already implemented (15 native + 1 delegated):** import/named, default, namespace, export,
no-unresolved, no-named-as-default, no-cycle, no-useless-path-segments, no-duplicates,
enforce-node-protocol-usage, n/file-extension-in-import, n/no-extraneous-import,
n/no-unsupported-features/node-builtins, prefer-global/process (prefer-global-this),
import/no-mutable-exports (delegated oxlint).

---

## Scope analysis — 9 rules

Need scope analysis beyond what oxc_semantic currently provides (component identity, prop flow, JSX semantics).

| Rule | Plugin | Why |
|------|--------|-----|
| `display-name` | react | Detect anonymous component exports |
| `jsx-handler-names` | react | Enforce `on*`/`handle*` naming for JSX handlers |
| `function-component-definition` | react | Enforce arrow vs function declaration |
| `no-render-return-value` | react | `ReactDOM.render()` return value used |
| `jsx-filename-extension` | react | Only allow JSX in `.jsx`/`.tsx` files |
| `jsx-fragments` | react | Enforce `<>` vs `React.Fragment` |
| `jsx-max-depth` | react | Limit JSX nesting depth |
| `boolean-prop-naming` | react | Enforce `is*`/`has*` for boolean props |
| `no-slowed-test` | playwright | Detect tests marked `.slow()` |

**Already implemented (11 native):** hook-use-state, jsx-no-undef, no-unknown-property,
no-find-dom-node, react-no-deprecated, destructuring-assignment, no-duplicate-slow,
no-unused-locators, valid-expect, valid-expect-in-promise, valid-describe-callback.

---

## Full regex parser — 4 rules

Need a regex AST parser to analyze regex structure (quantifiers, character classes, set operations).

| Rule | Plugin |
|------|--------|
| `no-invalid-regexp` | regexp |
| `strict` | regexp |
| `optimal-quantifier-concatenation` | regexp |
| `simplify-set-operations` | regexp |

**Already implemented (3 native heuristic):** no-empty-alternative, no-super-linear-backtracking,
no-misleading-unicode-character.

---

## Unicorn infra — 6 rules

Mixed infrastructure needs (regex parser, scope analysis, module resolution).

| Rule | Needs |
|------|-------|
| `better-regex` | Regex AST parser |
| `isolated-functions` | Scope + reference analysis |
| `import-style` | Module resolution |
| `no-unnecessary-polyfills` | Module resolution + target env |
| `no-unused-properties` | Cross-file usage analysis |
| `string-content` | Regex-based string content matching |

**Already implemented (1 native + 1 delegated):** consistent-function-scoping (native),
filename-case (delegated oxlint).

---

## Dropped

- **JSDoc type context (5 rules):** require-param-type, require-returns-type, require-property-type,
  require-next-type, require-throws-type — dropped, not actionable.
- **SKIP (139 rules):** Formatting-only, deprecated, superseded, or not applicable. See git history.
