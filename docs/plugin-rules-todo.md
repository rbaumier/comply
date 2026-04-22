# Plugin Rules — Status

Every rule from every plugin, categorized. Source of truth for coverage.

---

## Grand Summary

| Plugin | Total | Done | TODO | Later | Skip |
|--------|-------|------|------|-------|------|
| typescript-eslint | 134 | 72 | 0 | 55 | 7 |
| eslint-plugin-react | 104 | 42 | 0 | 13 | 49 |
| eslint-plugin-import | 46 | 25 | 0 | 14 | 7 |
| eslint-plugin-n | 43 | 18 | 0 | 14 | 11 |
| eslint-plugin-playwright | 58 | 35 | 0 | 6 | 17 |
| eslint-plugin-regexp | 82 | 55 | 0 | 7 | 20 |
| eslint-plugin-jsdoc | 66 | 34 | 0 | 5 | 27 |
| eslint-plugin-unicorn | 147 | 139 | 0 | 7 | 1 |
| **TOTAL** | **680** | **420** | **0** | **121** | **139** |

**All actionable rules implemented. 121 rules blocked on infrastructure (type checker, module resolution, scope analysis).**

---

## LATER — Rules needing infrastructure (121 total)

### Needs type checker (typescript-eslint: 55 rules)
await-thenable, consistent-return, consistent-type-exports, dot-notation, naming-convention, no-array-delete, no-base-to-string, no-confusing-void-expression, no-deprecated, no-duplicate-type-constituents, no-floating-promises, no-for-in-array, no-implied-eval, no-meaningless-void-operator, no-misused-promises, no-misused-spread, no-mixed-enums, no-redundant-type-constituents, no-unnecessary-boolean-literal-compare, no-unnecessary-condition, no-unnecessary-qualifier, no-unnecessary-template-expression, no-unnecessary-type-arguments, no-unnecessary-type-assertion, no-unnecessary-type-conversion, no-unnecessary-type-parameters, no-unsafe-argument, no-unsafe-assignment, no-unsafe-call, no-unsafe-enum-comparison, no-unsafe-member-access, no-unsafe-return, no-unsafe-unary-minus, no-useless-default-assignment, non-nullable-type-assertion-style, only-throw-error, prefer-destructuring, prefer-find, prefer-includes, prefer-nullish-coalescing, prefer-optional-chain, prefer-promise-reject-errors, prefer-readonly, prefer-readonly-parameter-types, prefer-reduce-type-parameter, prefer-regexp-exec, prefer-return-this-type, prefer-string-starts-ends-with, promise-function-async, related-getter-setter-pairs, require-array-sort-compare, require-await, restrict-plus-operands, restrict-template-expressions, return-await, strict-boolean-expressions, strict-void-return, switch-exhaustiveness-check, unbound-method, use-unknown-in-catch-callback-variable

### Needs module resolution (import: 14 + n: 14 = 28 rules)
import: no-unresolved, named, default, namespace, export, no-named-as-default, no-named-as-default-member, no-cycle, no-unused-modules, no-deprecated, no-extraneous-dependencies, order, no-useless-path-segments, enforce-node-protocol-usage
n: no-missing-import, no-missing-require, no-extraneous-import, no-extraneous-require, no-unpublished-bin, no-unpublished-import, no-unpublished-require, file-extension-in-import, no-unsupported-features/es-builtins, no-unsupported-features/es-syntax, no-unsupported-features/node-builtins, prefer-global/buffer, prefer-global/console, prefer-global/process

### Needs scope analysis (react: 13 + playwright: 6 = 19 rules)
react: display-name, jsx-no-undef, no-unknown-property, jsx-handler-names, hook-use-state, function-component-definition, no-deprecated, no-render-return-value, destructuring-assignment, jsx-filename-extension, jsx-fragments, jsx-max-depth, no-find-dom-node, boolean-prop-naming
playwright: no-duplicate-slow, no-slowed-test, no-unused-locators, valid-expect, valid-expect-in-promise, valid-describe-callback

### Needs full regex parser (regexp: 7 rules)
no-invalid-regexp, strict, optimal-quantifier-concatenation, simplify-set-operations

### Needs JSDoc type context (jsdoc: 5 rules)
require-param-type, require-returns-type, require-property-type, require-next-type, require-throws-type

### Needs unicorn infra (7 rules)
better-regex, consistent-function-scoping, isolated-functions, import-style, no-unnecessary-polyfills, no-unused-properties, string-content

---

## SKIP — Rules not applicable (139 total)

These rules are skipped for various reasons: formatting-only, deprecated, superseded by other rules, or not applicable to comply's use case. See git history for the full rationale.
