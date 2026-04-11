# Plugin Rules — Exhaustive TODO

Every rule from every plugin, categorized. Source of truth for what's missing.

---

## Grand Summary

| Plugin | Total | Done | TODO | Later | Skip |
|--------|-------|------|------|-------|------|
| typescript-eslint | 134 | 23 | 49 | 55 | 7 |
| eslint-plugin-react | 104 | 27 | 15 | 13 | 49 |
| eslint-plugin-import | 46 | 15 | 10 | 14 | 7 |
| eslint-plugin-n | 43 | 10 | 8 | 14 | 11 |
| eslint-plugin-playwright | 58 | 13 | 22 | 6 | 17 |
| eslint-plugin-regexp | 82 | 28 | 27 | 7 | 20 |
| eslint-plugin-jsdoc | 66 | 10 | 24 | 5 | 27 |
| eslint-plugin-unicorn | 147 | 139 | 0 | 7 | 1 |
| **TOTAL** | **680** | **265** | **155** | **121** | **139** |

**155 rules to implement now. 121 rules for later (need infra).**

---

## typescript-eslint — 49 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `adjacent-overload-signatures` | Overload signatures must be consecutive | TS+TSX | AstCheck |
| `ban-ts-comment` | Control @ts-ignore/@ts-expect-error usage | TS+TSX | AstCheck |
| `ban-tslint-comment` | Disallow // tslint: comments | TS+TSX | TextCheck |
| `class-literal-property-style` | Consistent class property literal style | TS+TSX | AstCheck |
| `class-methods-use-this` | Class methods must use this | TS+TSX | AstCheck |
| `consistent-generic-constructors` | new Map<K,V>() vs const m: Map<K,V> = new Map() | TS+TSX | AstCheck |
| `consistent-indexed-object-style` | Record<K,V> vs { [k: K]: V } | TS+TSX | AstCheck |
| `consistent-type-assertions` | Enforce as vs angle-bracket assertions | TS+TSX | AstCheck |
| `consistent-type-definitions` | type vs interface consistency | TS+TSX | AstCheck |
| `default-param-last` | Default parameters must be last | TS+TSX+JS | AstCheck |
| `explicit-function-return-type` | Require explicit return types | TS+TSX | AstCheck |
| `explicit-member-accessibility` | Require public/private/protected | TS+TSX | AstCheck |
| `explicit-module-boundary-types` | Explicit types on exported functions | TS+TSX | AstCheck |
| `init-declarations` | Require/disallow initialization in var declarations | TS+TSX+JS | AstCheck |
| `max-params` | Max function parameters | TS+TSX+JS+Rust | AstCheck |
| `member-ordering` | Consistent member declaration order | TS+TSX | AstCheck |
| `method-signature-style` | Property vs method signature in interfaces | TS+TSX | AstCheck |
| `no-array-constructor` | Disallow generic Array constructors | TS+TSX+JS | AstCheck |
| `no-dupe-class-members` | Disallow duplicate class members | TS+TSX+JS | AstCheck |
| `no-dynamic-delete` | Disallow delete with computed key | TS+TSX+JS | AstCheck |
| `no-empty-function` | Disallow empty functions | TS+TSX+JS+Rust | AstCheck |
| `no-extraneous-class` | Disallow static-only classes | TS+TSX+JS | AstCheck |
| `no-import-type-side-effects` | Enforce import type style | TS+TSX | AstCheck |
| `no-invalid-this` | Disallow this outside classes | TS+TSX+JS | AstCheck |
| `no-invalid-void-type` | Disallow void outside return/generic | TS+TSX | AstCheck |
| `no-loop-func` | Disallow functions in loops with unsafe refs | TS+TSX+JS+Rust | AstCheck |
| `no-magic-numbers` | Disallow magic numbers | TS+TSX+JS+Rust | AstCheck |
| `no-redeclare` | Disallow variable redeclaration | TS+TSX+JS | AstCheck |
| `no-restricted-imports` | Disallow specified imports | TS+TSX+JS | AstCheck |
| `no-restricted-types` | Disallow specified types | TS+TSX | AstCheck |
| `no-shadow` | Disallow variable shadowing | TS+TSX+JS+Rust | AstCheck |
| `no-this-alias` | Disallow this aliasing | TS+TSX+JS | AstCheck |
| `no-unnecessary-parameter-property-assignment` | Disallow redundant constructor property assignment | TS+TSX | AstCheck |
| `no-unused-expressions` | Disallow unused expressions | TS+TSX+JS | AstCheck |
| `no-unused-private-class-members` | Disallow unused private members | TS+TSX | AstCheck |
| `no-unused-vars` | Disallow unused variables | TS+TSX+JS | AstCheck |
| `no-use-before-define` | Disallow use before definition | TS+TSX+JS | AstCheck |
| `no-useless-constructor` | Disallow unnecessary constructors | TS+TSX+JS | AstCheck |
| `parameter-properties` | Consistent parameter property style | TS+TSX | AstCheck |
| `prefer-enum-initializers` | Require explicit enum initializers | TS+TSX | AstCheck |
| `prefer-for-of` | Prefer for-of when index not used | TS+TSX+JS | AstCheck |
| `prefer-function-type` | Function type over call signature interface | TS+TSX | AstCheck |
| `prefer-namespace-keyword` | namespace over module keyword | TS+TSX | AstCheck |
| `triple-slash-reference` | Disallow /// reference directives | TS+TSX | TextCheck |
| `unified-signatures` | Merge overloads that differ in one param | TS+TSX | AstCheck |

---

## eslint-plugin-react — 15 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `button-has-type` | `<button>` needs explicit type= | TSX+Vue | AstCheck |
| `jsx-key` | Missing key in iterators | TSX+Vue | AstCheck |
| `jsx-no-useless-fragment` | Unnecessary `<></>` wrapping | TSX | AstCheck |
| `jsx-pascal-case` | Components must be PascalCase | TSX | AstCheck |
| `jsx-props-no-spread-multi` | No spreading same identifier twice | TSX | AstCheck |
| `no-children-prop` | Use JSX children, not children= prop | TSX | AstCheck |
| `no-namespace` | No namespaced JSX `<Foo:Bar />` | TSX | AstCheck |
| `no-string-refs` | No string refs `ref="myRef"` | TSX | AstCheck |
| `no-unescaped-entities` | No raw `>` `<` `"` in JSX text | TSX+Vue | AstCheck |
| `self-closing-comp` | Self-close components without children | TSX+Vue | AstCheck |
| `no-invalid-html-attribute` | Invalid rel/charset values | TSX+Vue | AstCheck |
| `no-adjacent-inline-elements` | Adjacent inline elements need whitespace | TSX | AstCheck |
| `forward-ref-uses-ref` | forwardRef must use ref param | TSX | AstCheck |
| `no-typos` | Typos in React static properties | TSX | AstCheck |
| `jsx-no-bind` | No .bind()/arrows in JSX props | TSX | AstCheck |

---

## eslint-plugin-import — 10 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `exports-last` | Exports at end of file | TS+JS+TSX | TextCheck |
| `no-named-export` | Forbid named exports (default-only) | TS+JS+TSX | AstCheck |
| `no-commonjs` | Disallow require/module.exports | TS+JS+TSX | AstCheck |
| `no-amd` | Disallow AMD define/require | TS+JS+TSX | AstCheck |
| `no-webpack-loader-syntax` | No ! in import specifiers | TS+JS+TSX | TextCheck |
| `no-empty-named-blocks` | No import {} from 'x' | TS+JS+TSX | AstCheck |
| `no-dynamic-require` | No require(variable) | TS+JS+TSX | AstCheck |
| `dynamic-import-chunkname` | Require webpackChunkName on import() | TS+JS+TSX | AstCheck |
| `consistent-type-specifier-style` | Inline vs top-level type import | TS+TSX | AstCheck |
| `prefer-default-export` | Single export → make it default | TS+JS+TSX | AstCheck |

---

## eslint-plugin-n — 8 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `no-process-env` | Disallow process.env | TS+JS+TSX | AstCheck |
| `callback-return` | Require return after callback | TS+JS+TSX | AstCheck |
| `global-require` | No require() in non-top scope | TS+JS+TSX | AstCheck |
| `no-mixed-requires` | No mixing require types | TS+JS+TSX | AstCheck |
| `exports-style` | Consistent module.exports style | TS+JS+TSX | AstCheck |
| `hashbang` | Correct #! in executables | TS+JS+TSX | TextCheck |
| `no-exports-assign` | No exports = assignment | TS+JS+TSX | AstCheck |
| `no-top-level-await` | No top-level await in CJS | TS+JS+TSX | AstCheck |

---

## eslint-plugin-playwright — 22 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `expect-expect` | At least one expect per test | TS+JS+TSX | AstCheck |
| `max-expects` | Limit expects per test | TS+JS+TSX | AstCheck |
| `max-nested-describe` | Limit describe nesting | TS+JS+TSX | AstCheck |
| `no-commented-out-tests` | No commented test/it calls | TS+JS+TSX | TextCheck |
| `no-conditional-in-test` | No if/switch in test body | TS+JS+TSX | AstCheck |
| `no-duplicate-hooks` | No duplicate beforeEach/afterEach | TS+JS+TSX | AstCheck |
| `no-hooks` | No beforeEach/afterEach | TS+JS+TSX | AstCheck |
| `no-nested-step` | No nested test.step() | TS+JS+TSX | AstCheck |
| `no-nth-methods` | No .nth()/.first()/.last() | TS+JS+TSX | AstCheck |
| `no-skipped-test` | No test.skip() | TS+JS+TSX | AstCheck |
| `no-standalone-expect` | No expect outside test block | TS+JS+TSX | AstCheck |
| `no-useless-await` | No unnecessary await on PW methods | TS+JS+TSX | AstCheck |
| `no-useless-not` | No .not when positive matcher exists | TS+JS+TSX | AstCheck |
| `no-wait-for-selector` | No waitForSelector (use locators) | TS+JS+TSX | AstCheck |
| `no-wait-for-navigation` | No waitForNavigation (use waitForURL) | TS+JS+TSX | AstCheck |
| `prefer-comparison-matcher` | toBeGreaterThan over manual compare | TS+JS+TSX | AstCheck |
| `prefer-equality-matcher` | toEqual/toStrictEqual preference | TS+JS+TSX | AstCheck |
| `prefer-hooks-in-order` | Hook ordering convention | TS+JS+TSX | AstCheck |
| `prefer-hooks-on-top` | Hooks at top of describe | TS+JS+TSX | AstCheck |
| `prefer-strict-equal` | toStrictEqual over toEqual | TS+JS+TSX | AstCheck |
| `prefer-to-be` | toBe for primitives | TS+JS+TSX | AstCheck |
| `prefer-to-contain` | toContain over includes().toBe(true) | TS+JS+TSX | AstCheck |

---

## eslint-plugin-regexp — 27 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `no-contradiction-with-assertion` | Elements contradicting assertions | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-dupe-disjunctions` | Duplicate alternatives `a\|a` | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-misleading-capturing-group` | Misleading capturing groups | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-missing-g-flag` | Missing g flag in matchAll | TS+JS+TSX only | TextCheck |
| `no-optional-assertion` | Assertion in optional quantifier | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-potentially-useless-backreference` | Backrefs to unmatched groups | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-super-linear-move` | Quadratic-move quantifiers | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-assertions` | Always true/false assertions | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-backreference` | Useless backreferences | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-dollar-replacements` | Useless $ in replacements | TS+JS+TSX only | TextCheck |
| `confusing-quantifier` | Confusing quantifiers | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-empty-string-literal` | Empty string in v-flag class | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-extra-lookaround-assertions` | Unnecessary nested lookarounds | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-legacy-features` | Legacy RegExp features | TS+JS+TSX | TextCheck |
| `no-non-standard-flag` | Non-standard flags | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-trivially-nested-assertion` | Trivially nested assertions | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-trivially-nested-quantifier` | Nested quantifiers reducible | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-flag` | Unnecessary regex flags | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-quantifier` | Removable quantifiers | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-set-operand` | Unnecessary v-flag set operand | TS+JS+TSX+Rust+Vue | TextCheck |
| `no-useless-string-literal` | Single-char string in \q{} | TS+JS+TSX+Rust+Vue | TextCheck |
| `optimal-lookaround-quantifier` | Non-constant lookaround quantifier | TS+JS+TSX+Rust+Vue | TextCheck |
| `prefer-predefined-assertion` | Prefer \b over equivalent lookaround | TS+JS+TSX+Rust+Vue | TextCheck |
| `prefer-set-operation` | Prefer v-flag set operations | TS+JS+TSX | TextCheck |
| `informative-docs` | Disallow uninformative JSDoc | TS+JS+TSX+Rust | TextCheck |
| `reject-any-type` | Disallow any/* in JSDoc types | TS+JS+TSX | TextCheck |
| `reject-function-type` | Disallow Function in JSDoc types | TS+JS+TSX | TextCheck |

---

## eslint-plugin-jsdoc — 24 TODO

| Rule | Description | Scope | Backend |
|------|-------------|-------|---------|
| `check-property-names` | @property names match properties | TS+JS+TSX | TextCheck |
| `check-tag-names` | Valid/known JSDoc tag names | TS+JS+TSX | TextCheck |
| `check-template-names` | @template names match type params | TS+JS+TSX | TextCheck |
| `check-types` | JSDoc type preferences (String→string) | TS+JS+TSX | TextCheck |
| `check-values` | Valid @version/@since/@license values | TS+JS+TSX | TextCheck |
| `valid-types` | Syntactically valid JSDoc type expressions | TS+JS+TSX | TextCheck |
| `require-param-description` | @param needs description | TS+JS+TSX | TextCheck |
| `require-param-name` | @param needs name | TS+JS+TSX | TextCheck |
| `require-returns-description` | @returns needs description | TS+JS+TSX | TextCheck |
| `require-file-overview` | Require @file/@fileoverview | TS+JS+TSX | TextCheck |
| `require-hyphen-before-param-description` | Hyphen before param desc | TS+JS+TSX | TextCheck |
| `require-property` | @typedef must document @property | TS+JS+TSX | TextCheck |
| `require-property-description` | @property needs description | TS+JS+TSX | TextCheck |
| `require-property-name` | @property needs name | TS+JS+TSX | TextCheck |
| `require-rejects` | Async fn must document @rejects | TS+JS+TSX | TextCheck |
| `require-throws` | Throwing fn must document @throws | TS+JS+TSX+Rust | TextCheck |
| `require-yields` | Generator must document @yields | TS+JS+TSX | TextCheck |
| `require-yields-check` | @yields matches actual yields | TS+JS+TSX | TextCheck |
| `require-tags` | Require specific tags | TS+JS+TSX | TextCheck |
| `require-template` | Require @template for generics | TS+JS+TSX | TextCheck |
| `require-next-description` | @next needs description | TS+JS+TSX | TextCheck |
| `require-template-description` | @template needs description | TS+JS+TSX | TextCheck |
| `require-throws-description` | @throws needs description | TS+JS+TSX | TextCheck |
| `require-yields-description` | @yields needs description | TS+JS+TSX | TextCheck |

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

### Needs JSDoc type context (jsdoc: 7 rules)
require-param-type, require-returns-type, require-property-type, require-next-type, require-throws-type, require-yields-type

### Needs unicorn infra (7 rules)
better-regex, consistent-function-scoping, isolated-functions, import-style, no-unnecessary-polyfills, no-unused-properties, string-content
