# Plugin Rules — Blocked on Infrastructure

Audit: 2026-04-24. **83 rules** remaining (JSDoc rules dropped).

---

## Summary

| Infrastructure | Blocked | Already done | Notes |
|----------------|---------|--------------|-------|
| Type checker (typescript-eslint) | 51 | 9 | Needs tsc-level type resolution |
| Module resolution (import + n) | 13 | 15 | Needs extended ImportIndex |
| Scope analysis (react + playwright) | 9 | 11 | Partially unblockable with oxc_semantic |
| Full regex parser (regexp) | 4 | 3 | Needs regex AST parser |
| Unicorn infra | 6 | 1 | Mixed: regex, scope, module resolution |
| **Total** | **83** | **39** | |

---

## Type checker — 51 rules

All require type-aware analysis (type inference, type narrowing, or type resolution).

| Rule | Why |
|------|-----|
| `await-thenable` | Verify `await` operand is a thenable |
| `consistent-return` | Infer return type for consistency check |
| `dot-notation` | Check property exists on type |
| `naming-convention` | Apply conventions based on type kind |
| `no-base-to-string` | Detect inherited `.toString()` from Object |
| `no-confusing-void-expression` | `void` used as expression value |
| `no-deprecated` | Detect `@deprecated` via type metadata |
| `no-duplicate-type-constituents` | Simplify redundant unions/intersections |
| `no-floating-promises` | Promise not awaited (partial heuristic exists) |
| `no-for-in-array` | `for...in` on array type |
| `no-implied-eval` | `setTimeout(string)` via type check |
| `no-meaningless-void-operator` | `void expr` where expr is already void |
| `no-misused-promises` | Promise in boolean/void position |
| `no-misused-spread` | Spread on non-iterable |
| `no-mixed-enums` | Mixed string/number in enum |
| `no-redundant-type-constituents` | `string \| "a"` → `string` |
| `no-unnecessary-boolean-literal-compare` | `x === true` when x is boolean |
| `no-unnecessary-condition` | Condition always true/false per types |
| `no-unnecessary-qualifier` | Redundant namespace qualifier |
| `no-unnecessary-template-expression` | Template with constant type |
| `no-unnecessary-type-arguments` | Type arg same as default |
| `no-unnecessary-type-assertion` | `as T` when already type T |
| `no-unnecessary-type-conversion` | `.toString()` on string |
| `no-unnecessary-type-parameters` | Generic param used once |
| `no-unsafe-argument` | `any` arg passed to typed function |
| `no-unsafe-assignment` | Assign `any` to typed variable |
| `no-unsafe-call` | Call a value of type `any` |
| `no-unsafe-enum-comparison` | Compare enum with non-enum value |
| `no-unsafe-member-access` | Access member on `any` |
| `no-unsafe-return` | Return `any` from typed function |
| `no-unsafe-unary-minus` | `-x` on non-number type |
| `no-useless-default-assignment` | Default param redundant with type |
| `non-nullable-type-assertion-style` | Prefer `!` over `as NonNullable<T>` |
| `only-throw-error` | Throw non-Error (partial heuristic exists) |
| `prefer-find` | `.filter()[0]` → `.find()` (needs array type) |
| `prefer-nullish-coalescing` | `\|\|` → `??` (needs nullable check) |
| `prefer-optional-chain` | `a && a.b` → `a?.b` (needs types) |
| `prefer-readonly` | Private property never reassigned |
| `prefer-readonly-parameter-types` | Unmutated params → readonly type |
| `prefer-reduce-type-parameter` | Explicit type param for `.reduce<T>()` |
| `prefer-return-this-type` | Return `this` instead of class name |
| `promise-function-async` | Function returning Promise → `async` |
| `related-getter-setter-pairs` | Getter/setter type mismatch |
| `require-array-sort-compare` | `.sort()` without comparator on non-string[] |
| `require-await` | `async` function without `await` |
| `restrict-plus-operands` | `+` on incompatible types |
| `restrict-template-expressions` | `${}` with non-stringifiable type |
| `return-await` | Check if `return await` is necessary |
| `strict-boolean-expressions` | Force explicit boolean in conditions |
| `switch-exhaustiveness-check` | Switch on enum without exhaustive default |
| `unbound-method` | Class method passed without bind |

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

**Already implemented (15):** import/named, default, namespace, export, no-unresolved, no-named-as-default, no-cycle, no-useless-path-segments, no-duplicates, enforce-node-protocol-usage, n/file-extension-in-import, n/no-extraneous-import, n/no-unsupported-features/node-builtins, prefer-global/process (prefer-global-this), import-no-cycle.

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

**Already implemented (11):** hook-use-state, jsx-no-undef, no-unknown-property, no-find-dom-node, react-no-deprecated, destructuring-assignment, no-duplicate-slow, no-unused-locators, valid-expect, valid-expect-in-promise, valid-describe-callback.

---

## Full regex parser — 4 rules

Need a regex AST parser to analyze regex structure (quantifiers, character classes, set operations).

| Rule | Plugin |
|------|--------|
| `no-invalid-regexp` | regexp |
| `strict` | regexp |
| `optimal-quantifier-concatenation` | regexp |
| `simplify-set-operations` | regexp |

**Already implemented (3):** no-empty-alternative, no-super-linear-backtracking, no-misleading-unicode-character (via heuristics).

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

**Already implemented (1):** consistent-function-scoping.

---

## Dropped

- **JSDoc type context (5 rules):** require-param-type, require-returns-type, require-property-type, require-next-type, require-throws-type — dropped, not actionable.
- **SKIP (139 rules):** Formatting-only, deprecated, superseded, or not applicable. See git history.
