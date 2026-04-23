# Rules Backlog — Exploration des plugins ESLint

Généré le 2026-04-23 à partir de 52 plugins dans `explore/`.
Mis à jour avec analyse de couverture comply et scope langages.

Légende:
- ✅ **Recommandé** — High-value, AST pur, facile à implémenter
- ⚠️ **Optionnel** — Niche ou config-heavy
- comply: ✓ (existe), ≈ (partiel), ✗ (n'existe pas)

---

## Security

### eslint-plugin-no-secrets
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-secrets` | Flag strings avec haute entropie Shannon ou matching patterns secrets | ✅ | ≈ `no-hardcoded-secret` (patterns OK, manque entropy) | TS/JS/TSX/Vue/Rust |
| `no-pattern-match` | Flag strings matching regex user-defined | ⚠️ | ✗ (config-heavy) | — |

**Amélioration**: Ajouter Shannon entropy scan à `no-hardcoded-secret` + patterns GCP/Facebook/Twitter/Heroku.

### eslint-plugin-no-unsanitized
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `method` | Interdit `insertAdjacentHTML`, `document.write`, `Range.createContextualFragment` non-sanitisés | ✅ | ✗ | TS/JS/TSX/Vue |
| `property` | Interdit assignments à `innerHTML`, `outerHTML`, `srcdoc` | ✅ | ✗ | TS/JS/TSX/Vue |

**Note**: `no-dangerously-set-inner-html` couvre React JSX uniquement.

### eslint-plugin-pii
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-dob` | Interdit patterns date-of-birth | ⚠️ | ✗ (FP élevé) | — |
| `no-email` | Interdit emails dans strings/comments | ⚠️ | ✗ | TS/JS/TSX/Vue/Rust |
| `no-ip` | Interdit IPv4/IPv6 | ⚠️ | ✓ `no-hardcoded-ip` | TS/JS/TSX/Vue/Rust |
| `no-phone-number` | Interdit numéros de téléphone | ⚠️ | ✗ (FP élevé) | — |

### eslint-plugin-sdl
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-electron-node-integration` | Flag `nodeIntegration: true` dans BrowserWindow | ✅ | ✗ | TS/JS/TSX |
| `no-msapp-exec-unsafe` | Flag `MSApp.execUnsafeLocalFunction` | ⚠️ | ✗ (MS legacy) | — |
| `no-winjs-html-unsafe` | Flag `WinJS.Utilities.setInnerHTMLUnsafe` | ⚠️ | ✗ (MS legacy) | — |

### eslint-plugin-security-node
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `detect-absence-of-name-option-in-express-session` | Flag `session({...})` sans `name` key | ✅ | ✗ | TS/JS |
| `detect-crlf` | Flag logs concaténant user input sans strip CRLF | ⚠️ | ✗ (FP élevé) | — |
| `detect-dangerous-redirects` | Flag `res.redirect(userInput)` | ⚠️ | ✓ `no-open-redirect` (supérieur) | TS/JS/TSX |
| `detect-html-injection` | Flag `res.send`/`res.write` avec concat user input | ⚠️ | ✗ | TS/JS/TSX/Vue |
| `detect-improper-exception-handling` | Flag `catch (e) {}` vide | ✅ | ✗ | TS/JS/TSX/Rust |
| `detect-option-multiplestatements-in-mysql` | Flag `createConnection({multipleStatements: true})` | ✅ | ✗ | TS/JS |
| `detect-option-rejectunauthorized-in-nodejs-httpsrequest` | Flag `https.request({rejectUnauthorized: false})` | ✅ | ≈ `no-weak-ssl` (manque ce cas) | TS/JS |
| `detect-option-unsafe-in-serialize-javascript-npm-package` | Flag `serialize(x, {unsafe: true})` | ✅ | ✗ | TS/JS |
| `detect-possible-timing-attacks` | Flag `===`/`!==` sur `password`, `token`, `secret` | ✅ | ✓ `no-timing-attack` (supérieur) | TS/JS/TSX/Rust |
| `detect-runinthiscontext-method-in-nodes-vm` | Flag `vm.runInThisContext(nonLiteral)` | ✅ | ≈ `no-eval` (manque vm.*) | TS/JS |
| `detect-security-missconfiguration-cookie` | Flag `res.cookie(..., {httpOnly: false})` | ✅ | ≈ `hono_cookie_no_*` (Hono only) | TS/JS |
| `detect-sql-injection` | Flag `db.query(concatOrTemplate)` | ⚠️ | ✓ `db-no-string-concat-sql` (supérieur) | TS/JS/TSX/Rust/Vue |
| `detect-unhandled-async-errors` | Flag promise chains sans `.catch` | ⚠️ | ✓ oxlint `no-floating-promises` | TS/TSX |
| `detect-unhandled-event-errors` | Flag EventEmitter sans listener `'error'` | ⚠️ | ✗ (FP élevé) | — |
| `disable-ssl-across-node-server` | Flag `process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'` | ✅ | ≈ `no-weak-ssl` (manque ce cas) | TS/JS |

**Améliorations**:
- Étendre `no-weak-ssl` → `rejectUnauthorized: false` + `NODE_TLS_REJECT_UNAUTHORIZED='0'`
- Étendre `no-eval` → `vm.runInThisContext/runInContext/runInNewContext`
- Généraliser `hono_cookie_*` → Express/Fastify aussi

### eslint-plugin-react-security
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-find-dom-node` | Flag `ReactDOM.findDOMNode(...)` | ✅ | ✗ | TS/JS/TSX |

---

## React

### eslint-plugin-react-perf
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `jsx-no-jsx-as-prop` | Flag JSX passés comme prop | ✅ | ✗ | TSX |
| `jsx-no-new-array-as-prop` | Flag array literals inline comme prop | ✅ | ✗ | TSX |
| `jsx-no-new-function-as-prop` | Flag inline functions comme prop | ✅ | ≈ `react-jsx-no-bind` (bind only) | TSX |
| `jsx-no-new-object-as-prop` | Flag object literals inline comme prop | ✅ | ✗ | TSX |

**Amélioration**: Étendre `react-jsx-no-bind` en `react-jsx-no-new-function-as-prop` (inclure arrows, function expressions).

### eslint-plugin-react-hook-form
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `destructuring-formstate` | Force destructuring de `formState` | ⚠️ | ✗ | TSX |
| `no-access-control` | Interdit lecture de props sur `control` | ⚠️ | ✗ | TSX |
| `no-nested-object-setvalue` | Interdit object literal comme arg de `setValue` | ⚠️ | ✗ | TSX |
| `no-use-watch` | Préfère `useWatch` over `watch()` | ⚠️ | ✗ | TSX |

### eslint-plugin-react-you-might-not-need-an-effect
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-empty-effect` | Flag `useEffect` avec body vide | ✅ | ✗ | TSX |
| `no-initialize-state` | Flag `setState` dans `useEffect` deps vides | ✅ | ✗ | TSX |
| `no-derived-state` | Flag `setState` computed depuis props/state | ⚠️ | ≈ `react-no-derived-state-in-effect` | TSX |
| `no-adjust-state-on-prop-change` | Flag `setState` quand deps contiennent prop | ⚠️ | ✗ | TSX |
| `no-chain-state-updates` | Flag cascading state updates | ⚠️ | ✗ | TSX |
| `no-event-handler` | Flag useEffect agissant comme event handler | ⚠️ | ✗ | TSX |
| `no-pass-live-state-to-parent` | Flag callback prop dans useEffect | ⚠️ | ✗ | TSX |
| `no-pass-data-to-parent` | Flag passing data à parent via useEffect | ⚠️ | ✗ | TSX |
| `no-reset-all-state-on-prop-change` | Flag useEffect resettant states — use `key` | ⚠️ | ✗ | TSX |

---

## Functional Programming

### eslint-plugin-fp
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-class` | Interdit `class` | ⚠️ | ≈ `no_class_inheritance`, `ts_no_extraneous_class` | TS/JS/TSX |
| `no-events` | Interdit `require('events')` | ⚠️ | ✗ | TS/JS/TSX |
| `no-get-set` | Interdit getters/setters | ⚠️ | ✗ | TS/JS/TSX |
| `no-let` | Interdit `let` | ⚠️ | ✗ (oxlint `prefer-const`) | TS/JS/TSX |
| `no-loops` | Interdit loops | ⚠️ | ≈ `no_for_loop`, `no_for_in_iterable` | TS/JS/TSX/Rust |
| `no-mutating-assign` | Interdit `Object.assign(target, ...)` mutant | ✅ | ✗ | TS/JS/TSX |
| `no-mutating-methods` | Interdit `push`, `sort`, `splice`, etc. | ⚠️ | ≈ `no_array_sort_mutation` (sort only) | TS/JS/TSX |
| `no-mutation` | Interdit reassignments | ⚠️ | ✗ (trop strict) | — |
| `no-nil` | Interdit `null`/`undefined` literals | ⚠️ | ≈ `no_null` (null only) | TS/JS/TSX |
| `no-proxy` | Interdit `new Proxy(...)` | ⚠️ | ✗ | TS/JS/TSX |
| `no-this` | Interdit `this` | ⚠️ | ≈ `no_this_assignment`, `react_no_this_in_sfc` | TS/JS/TSX |
| `no-unused-expression` | Interdit expression statements ignorés | ⚠️ | ✓ oxlint `no-unused-expressions` | TS/JS/TSX |
| `no-valueof-field` | Interdit `valueOf` property | ✅ | ✗ | TS/JS/TSX |

**Amélioration**: Étendre `no_array_sort_mutation` → inclure push/pop/shift/unshift/splice/fill/reverse.

### eslint-plugin-functional
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `functional-parameters` | Enforce functional parameter style | ⚠️ | ≈ `no_arguments_usage`, `bool_param_default` | TS/JS/TSX |
| `immutable-data` | Interdit mutation object/array | ⚠️ | ✗ (trop strict) | — |
| `no-class-inheritance` | Interdit `extends` | ⚠️ | ✓ `no_class_inheritance` | TS/JS/TSX |
| `no-classes` | Interdit class | ⚠️ | ≈ (partiel) | TS/JS/TSX |
| `no-let` | Interdit `let` | ⚠️ | ✗ | TS/JS/TSX |
| `no-loop-statements` | Interdit all loops | ⚠️ | ≈ `no_for_loop` | TS/JS/TSX |
| `no-mixed-types` | Interdit type mélangeant props et methods | ✅ | ✗ | TS/TSX |
| `no-promise-reject` | Interdit `Promise.reject(...)` | ⚠️ | ✓ `no_promise_reject` | TS/JS/TSX |
| `no-this-expressions` | Interdit `this` | ⚠️ | ≈ (partiel) | TS/JS/TSX |
| `no-throw-statements` | Interdit `throw` | ⚠️ | ✓ `no_throw` | TS/JS/TSX |
| `no-try-statements` | Interdit `try/catch` | ⚠️ | ✓ `no_try_statements` | TS/JS/TSX |
| `prefer-property-signatures` | Require `foo: () => T` over `foo(): T` | ✅ | ✗ | TS/TSX |

---

## Architecture

### eslint-plugin-boundaries
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `dependencies` | Interdit imports entre éléments selon contraintes | ✅ | ≈ `layer_import_boundary` (hexagonal-specific) | TS/JS/TSX/Vue |
| `no-private` | Interdit importer module privé d'autre élément | ✅ | ≈ `api_import_from_public_index` | TS/JS/TSX/Vue |
| `no-unknown` | Interdit imports vers fichiers ne matchant aucun descripteur | ⚠️ | ✗ (config-heavy) | — |

**Amélioration**: Généraliser `layer_import_boundary` avec config elements + allow/disallow matrix.

### eslint-plugin-fsd-lint
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-cross-slice-dependency` | Deux slices même couche ne s'importent pas | ✅ | ✗ | TS/JS/TSX/Vue |
| `no-global-store-imports` | Interdit import store global depuis couches bas | ✅ | ✗ | TS/JS/TSX/Vue |
| `no-relative-imports` | Interdit imports relatifs traversant slices | ✅ | ✗ | TS/JS/TSX/Vue |
| `no-ui-in-business-logic` | Interdit `ui/` depuis `model/`, `api/`, `lib/` | ✅ | ✗ | TS/JS/TSX/Vue |

### eslint-plugin-barrel-files
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `avoid-barrel-files` | Signale fichier re-export only | ✅ | ✗ | TS/JS/TSX/Vue |
| `avoid-importing-barrel-files` | Interdit importer depuis baril | ✅ | ✗ | TS/JS/TSX/Vue |
| `avoid-namespace-import` | Interdit `import * as X` | ⚠️ | ✓ `no_namespace_import` | TS/JS/TSX |
| `avoid-re-export-all` | Interdit `export * from` | ✅ | ✗ | TS/JS/TSX/Vue |

---

## i18n

### eslint-plugin-i18n-json
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `valid-json` | Ensure JSON parses | ⚠️ | ✗ | JSON |
| `valid-message-syntax` | Validate ICU MessageFormat | ⚠️ | ✗ | JSON |
| `identical-keys` | Compare keys vs reference locale | ⚠️ | ✗ | JSON |
| `identical-placeholders` | Compare ICU placeholders | ⚠️ | ✗ | JSON |

### eslint-plugin-i18next
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-literal-string` | Flag literal strings non wrappées | ✅ | ≈ `i18n_no_hardcoded_string_in_jsx` (JSX only) | TS/JS/TSX/Vue |

**Amélioration**: Étendre `i18n_no_hardcoded_string_in_jsx` hors JSX (tous les string literals user-facing).

---

## Testing

### eslint-plugin-vitest
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `consistent-test-it` | Enforce `test` vs `it` | ⚠️ | ✗ | TS/JS/TSX |
| `consistent-test-filename` | Require test filenames match pattern | ⚠️ | ✗ | TS/JS/TSX |
| `consistent-vitest-vi` | Enforce `vi` vs `vitest` namespace | ⚠️ | ✗ | TS/JS/TSX |
| `consistent-each-for` | Prefer `.each(...)` over loops | ✅ | ≈ `testing_prefer_test_each` | TS/JS/TSX |
| `hoisted-apis-on-top` | Require `vi.hoisted` at top | ⚠️ | ✗ | TS/JS/TSX |
| `max-expects` | Cap number of `expect` per test | ⚠️ | ≈ `playwright_max_expects` | TS/JS/TSX |
| `no-alias-methods` | Disallow Jest alias matchers | ✅ | ✗ | TS/JS/TSX |
| `no-conditional-tests` | Disallow conditionally-defined tests | ✅ | ✗ | TS/JS/TSX |
| `no-done-callback` | Disallow `done`-style callbacks | ✅ | ✗ | TS/JS/TSX |
| `no-duplicate-hooks` | Disallow duplicate hooks | ✅ | ≈ `playwright_no_duplicate_hooks` | TS/JS/TSX |
| `no-hooks` | Disallow hooks entirely | ⚠️ | ≈ `playwright_no_hooks` | TS/JS/TSX |
| `no-identical-title` | Disallow duplicate titles | ✅ | ✗ | TS/JS/TSX |
| `no-import-node-test` | Disallow importing `node:test` | ✅ | ✗ | TS/JS/TSX |
| `no-importing-vitest-globals` | Disallow importing when globals enabled | ⚠️ | ✗ | TS/JS/TSX |
| `no-interpolation-in-snapshots` | Disallow interpolation in snapshots | ✅ | ✗ | TS/JS/TSX |
| `no-large-snapshots` | Warn when snapshots exceed size | ⚠️ | ✗ | TS/JS/TSX |
| `no-mocks-import` | Disallow importing from `__mocks__` | ✅ | ✗ | TS/JS/TSX |
| `no-restricted-matchers` | Disallow configured matchers | ⚠️ | ✗ | TS/JS/TSX |
| `no-restricted-vi-methods` | Disallow configured `vi.*` methods | ⚠️ | ✗ | TS/JS/TSX |
| `no-test-prefixes` | Disallow `f*`/`x*` prefix forms | ✅ | ≈ `no_focused_test` (partial) | TS/JS/TSX |
| `no-test-return-statement` | Disallow `return` in test body | ✅ | ✗ | TS/JS/TSX |
| `prefer-called-exactly-once-with` | Prefer `toHaveBeenCalledExactlyOnceWith` | ✅ | ✗ | TS/JS/TSX |
| `prefer-called-with` | Prefer `toHaveBeenCalledWith` | ✅ | ✗ | TS/JS/TSX |
| `prefer-comparison-matcher` | Prefer `toBeGreaterThan`/`toBeLessThan` | ✅ | ≈ `playwright_prefer_comparison_matcher` | TS/JS/TSX |
| `prefer-each` | Prefer `.each` over forEach | ✅ | ✓ `testing_prefer_test_each` | TS/JS/TSX |
| `prefer-equality-matcher` | Prefer `.toBe`/`.toEqual` | ✅ | ≈ `playwright_prefer_equality_matcher` | TS/JS/TSX |
| `prefer-expect-assertions` | Require `expect.assertions(n)` | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-expect-resolves` | Prefer `expect(promise).resolves` | ✅ | ✗ | TS/JS/TSX |
| `prefer-hooks-in-order` | Enforce canonical hook order | ✅ | ≈ `playwright_prefer_hooks_in_order` | TS/JS/TSX |
| `prefer-hooks-on-top` | Hooks must appear before tests | ✅ | ≈ `playwright_prefer_hooks_on_top` | TS/JS/TSX |
| `prefer-lowercase-title` | Enforce lowercase titles | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-mock-promise-shorthand` | Prefer `.mockResolvedValue` | ✅ | ✗ | TS/JS/TSX |
| `prefer-mock-return-shorthand` | Prefer `.mockReturnValue` | ✅ | ✗ | TS/JS/TSX |
| `prefer-snapshot-hint` | Require hint on `toMatchSnapshot` | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-spy-on` | Prefer `vi.spyOn` over `vi.fn()` | ✅ | ✗ | TS/JS/TSX |
| `prefer-strict-equal` | Prefer `.toStrictEqual` | ✅ | ≈ `playwright_prefer_strict_equal` | TS/JS/TSX |
| `prefer-to-be` | Prefer `.toBe` for primitives | ✅ | ≈ `playwright_prefer_to_be` | TS/JS/TSX |
| `prefer-to-be-falsy` | Prefer `.toBeFalsy` | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-to-be-object` | Prefer `.toBeInstanceOf(Object)` | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-to-be-truthy` | Prefer `.toBeTruthy` | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-to-contain` | Prefer `.toContain` | ✅ | ≈ `playwright_prefer_to_contain` | TS/JS/TSX |
| `prefer-to-have-length` | Prefer `.toHaveLength` | ✅ | ✗ | TS/JS/TSX |
| `prefer-todo` | Prefer `test.todo` | ✅ | ✗ | TS/JS/TSX |
| `require-hook` | Require side-effects inside hooks | ✅ | ✗ | TS/JS/TSX |
| `require-to-throw-message` | Require message on `.toThrow()` | ✅ | ✗ | TS/JS/TSX |
| `require-top-level-describe` | Require tests wrapped in describe | ⚠️ | ✗ | TS/JS/TSX |
| `valid-describe-callback` | `describe` callback rules | ✅ | ✗ | TS/JS/TSX |
| `valid-expect` | `expect` must be called correctly | ✅ | ≈ `playwright_no_standalone_expect` (partial) | TS/JS/TSX |
| `valid-title` | Titles must be non-empty strings | ✅ | ≈ `testing_no_and_in_test_name` (partial) | TS/JS/TSX |

**Opportunité majeure**: Généraliser `playwright_*` → règles testing génériques (jest/vitest/playwright).

### eslint-plugin-playwright
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-duplicate-slow` | Disallow duplicate `test.slow()` | ✅ | ✗ | TS/JS |
| `no-eval` | Disallow `page.$eval` / `page.$$eval` | ✅ | ≈ `playwright_no_element_handle` (partial) | TS/JS |
| `no-focused-test` | Disallow `test.only` | ✅ | ✓ `no_focused_test` | TS/JS/TSX |
| `no-get-by-title` | Disallow `getByTitle` (a11y smell) | ⚠️ | ✗ | TS/JS |
| `no-page-pause` | Disallow `page.pause()` | ✅ | ✓ `playwright_no_page_pause` | TS/JS |
| `no-restricted-locators` | Disallow configured locators | ⚠️ | ✗ | TS/JS |
| `no-restricted-matchers` | Disallow configured matchers | ⚠️ | ✗ | TS/JS |
| `no-restricted-roles` | Disallow configured roles | ⚠️ | ✗ | TS/JS |
| `no-slowed-test` | Disallow `test.slow()` | ⚠️ | ✗ | TS/JS |
| `no-unused-locators` | Disallow unused locators | ✅ | ✗ | TS/JS |
| `no-wait-for-timeout` | Disallow `page.waitForTimeout` | ✅ | ✗ | TS/JS |
| `prefer-locator` | Prefer `locator` over `page.$` | ✅ | ✓ `playwright_no_raw_locators` | TS/JS |
| `prefer-to-have-count` | Prefer `.toHaveCount(n)` | ✅ | ✗ | TS/JS |
| `require-soft-assertions` | Require `expect.soft` | ⚠️ | ✗ | TS/JS |
| `require-tags` | Require tests include tags | ⚠️ | ✗ | TS/JS |
| `require-to-pass-timeout` | Require timeout on `.toPass()` | ✅ | ✗ | TS/JS |
| `valid-test-tags` | Validate `{ tag: [...] }` | ✅ | ✗ | TS/JS |

---

## Tailwind

### eslint-plugin-tailwindcss
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `classnames-order` | Enforce canonical class ordering | ⚠️ | ✗ (prettier territory) | — |
| `enforces-negative-arbitrary-values` | Flag `-top-[1px]` → `top-[-1px]` | ✅ | ✗ | TS/JS/TSX/Vue/HTML |
| `enforces-shorthand` | Replace longhand by shorthand (`p-4`) | ✅ | ≈ `tailwind_prefer_size_shorthand` (size only) | TS/JS/TSX/Vue/HTML |
| `migration-from-tailwind-2` | Detect obsolete v2 class names | ✅ | ✗ | TS/JS/TSX/Vue/HTML |
| `no-arbitrary-value` | Forbid any `[...]` arbitrary value | ⚠️ | ≈ `tailwind_no_arbitrary_z_index` (z-index only) | TS/JS/TSX/Vue/HTML |
| `no-unnecessary-arbitrary-value` | Flag `[16px]` when token exists | ⚠️ | ✗ (config-heavy) | — |

**Amélioration**: Étendre `tailwind_prefer_size_shorthand` → padding/margin/rounded/border shorthands.

### eslint-plugin-better-tailwindcss
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `enforce-canonical-classes` | Rewrite to canonical form | ⚠️ | ✗ (version-coupled) | — |
| `enforce-consistent-class-order` | Alphabetic or logical ordering | ⚠️ | ✗ (prettier territory) | — |
| `enforce-consistent-important-position` | `!` prefix vs suffix | ✅ | ≈ `tailwind_no_important_modifier` (stricter) | TS/JS/TSX/Vue/HTML |
| `enforce-consistent-variable-syntax` | `(--var)` vs `[--var]` syntax | ✅ | ✗ | TS/JS/TSX/Vue/HTML |
| `enforce-consistent-variant-order` | Variant order in `md:hover:...` | ✅ | ✗ | TS/JS/TSX/Vue/HTML |
| `no-deprecated-classes` | Flag v4-deprecated utilities | ✅ | ✗ | TS/JS/TSX/Vue/HTML |
| `no-restricted-classes` | User-configured disallow list | ⚠️ | ✗ | TS/JS/TSX/Vue/HTML |
| `no-unnecessary-whitespace` | Collapse extra spaces | ✅ | ✗ | TS/JS/TSX/Vue/HTML |

---

## Utility

### eslint-plugin-antfu
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `import-dedupe` | Remove duplicate specifiers `{ a, a }` | ✅ | ≈ `no_duplicate_imports` (declarations only) | TS/JS/TSX/Rust |
| `no-import-dist` | Forbid importing from `dist/` | ✅ | ✗ | TS/JS/TSX |
| `no-import-node-modules-by-path` | Forbid `/node_modules/` imports | ✅ | ✗ | TS/JS/TSX |
| `no-top-level-await` | Forbid top-level `await` | ✅ | ✓ `node_no_top_level_await` | TS/JS/TSX |
| `no-ts-export-equal` | Forbid `export = ...` | ✅ | ✗ | TS/TSX |
| `top-level-function` | Enforce function declarations | ⚠️ | ✗ | TS/JS/TSX |

**Amélioration**: Étendre `no_duplicate_imports` → dedupe named specifiers within single declaration.

### eslint-plugin-better
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `explicit-return` | Every function must end with `return` | ⚠️ | ✗ | — |
| `must-return` | Every function must return value | ⚠️ | ✗ | — |
| `no-classes` | Forbid `class` | ⚠️ | ≈ (partial) | TS/JS/TSX |
| `no-deletes` | Forbid `delete` | ⚠️ | ≈ `no_array_delete`, `ts_no_dynamic_delete` | TS/JS/TSX |
| `no-exceptions` | Forbid `throw`/`try` | ⚠️ | ✓ `no_throw` + `no_try_statements` | TS/JS/TSX |
| `no-fors` | Forbid `for` loops | ⚠️ | ≈ `no_for_loop` | TS/JS/TSX |
| `no-function-expressions` | Forbid `function()` expressions | ⚠️ | ✗ | — |
| `no-instanceofs` | Forbid `instanceof` | ⚠️ | ≈ `no_instanceof_builtins` (narrower) | TS/JS/TSX |
| `no-new` | Forbid `new` | ⚠️ | ✗ (too strict) | — |
| `no-nulls` | Forbid `null` | ⚠️ | ✓ `no_null` | TS/JS/TSX |
| `no-reassigns` | Forbid reassignment | ⚠️ | ✗ | — |
| `no-switches` | Forbid `switch` | ⚠️ | ✗ | — |
| `no-this` | Forbid `this` | ⚠️ | ≈ (partial) | TS/JS/TSX |
| `no-undefined` | Forbid `undefined` | ⚠️ | ≈ `no_undefined_argument`, `no_undefined_assignment` | TS/JS/TSX |
| `no-variable-declaration` | Forbid `var`/`let` | ⚠️ | ✗ | — |
| `no-whiles` | Forbid `while` loops | ⚠️ | ✗ | — |

### eslint-plugin-clsx
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `forbid-array-expressions` | Forbid array literals in `clsx(...)` | ⚠️ | ✗ | TSX |
| `forbid-false-inside-object-expressions` | Forbid `{ key: false }` | ⚠️ | ✗ | TSX |
| `forbid-true-inside-object-expressions` | Forbid `{ key: true }` | ⚠️ | ✗ | TSX |
| `no-redundant-clsx` | Forbid `clsx("single-string")` | ✅ | ✗ | TS/JS/TSX |
| `no-spreading` | Forbid spread `clsx(...props)` | ⚠️ | ✗ | TSX |
| `prefer-logical-over-objects` | Prefer `cond && 'cls'` | ⚠️ | ✗ | TSX |
| `prefer-merged-neighboring-elements` | Merge adjacent strings | ⚠️ | ✗ | TSX |
| `prefer-objects-over-logical` | Prefer `{ cls: cond }` | ⚠️ | ✗ | TSX |

### eslint-plugin-de-morgan
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-negated-conjunction` | Forbid `!(a && b)` | ✅ | ✓ `de_morgan_simplify` | TS/JS/TSX/Rust |
| `no-negated-disjunction` | Forbid `!(a \|\| b)` | ✅ | ✓ `de_morgan_simplify` | TS/JS/TSX/Rust |

**Note**: `de_morgan_simplify` couvre les deux cas. Rien à faire.

### eslint-plugin-depend
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `ban-dependencies` | Ban imports with presets (`microutilities`, `native`) | ✅ | ≈ `ts_no_restricted_imports` (no presets) | TS/JS/TSX |

**Amélioration**: Ajouter `presets` à `ts_no_restricted_imports` avec table `module-replacements`.

### eslint-plugin-etc
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-assign-mutated-array` | Forbid `const x = arr.sort()` | ✅ | ✗ | TS/JS/TSX |
| `no-const-enum` | Forbid `const enum` | ✅ | ✗ | TS/TSX |
| `no-foreach` | Forbid `.forEach()` | ⚠️ | ✗ | — |
| `no-implicit-any-catch` | Require `catch (e: unknown)` | ✅ | ✗ | TS/TSX |
| `prefer-less-than` | Prefer `a < b` over `b > a` | ✅ | ✗ | TS/JS/TSX/Rust |
| `throw-error` | Forbid `throw` non-Error | ✅ | ≈ oxlint `no-throw-literal` | TS/JS/TSX |

---

## Exception Handling

### eslint-plugin-exception-handling
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `might-throw` | Signale appels qui peuvent throw | ⚠️ | ✗ (inter-proc heavy) | — |
| `no-unhandled` | Comme might-throw sans try/catch | ⚠️ | ✗ | — |
| `use-error-cause` | Force `{cause: e}` sur re-throw | ✅ | ✓ `error_without_cause` | TS/JS/TSX/Rust |

---

## Listeners

### eslint-plugin-listeners
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-missing-remove-event-listener` | Chaque add doit avoir remove | ✅ | ≈ `vue_require_lifecycle_cleanup` (Vue only) | TS/JS/TSX/Vue |
| `matching-remove-event-listener` | add/remove même handler | ✅ | ≈ `no_invalid_remove_event_listener` (partial) | TS/JS/TSX/Vue |
| `no-inline-function-event-listener` | Interdit inline listener | ✅ | ✗ | TS/JS/TSX/Vue |

---

## Proper Arrows

### eslint-plugin-proper-arrows
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `params` | Contrôle params | ⚠️ | ✗ | — |
| `name` | Arrow assignée à variable nommée | ⚠️ | ✗ | — |
| `where` | Interdit arrows dans certains contextes | ⚠️ | ✗ | — |
| `return` | Contrôle type de retour | ⚠️ | ✗ | — |
| `this` | `this` seulement dans arrow nested dans function | ✅ | ✗ | TS/JS/TSX |

---

## Financial

### eslint-plugin-financial
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-division` | Interdit `/` | ⚠️ | ✗ (too strict) | — |
| `no-float-calculation` | Interdit ops sur décimaux | ⚠️ | ≈ `rust_no_float_for_money`, `sql_no_float_for_money` | TS/JS/TSX/Rust/SQL |

---

## Misc

### eslint-plugin-no-floating-promise
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-floating-promise` | Flag async calls ni awaited, ni chainés | ✅ | ✓ oxlint `no-floating-promises` | TS/TSX |

### eslint-plugin-require-path-exists
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `exists` | Import/require résout vers fichier | ⚠️ | ✗ | TS/JS/TSX |
| `notEmpty` | Forbid empty import path | ✅ | ✗ | TS/JS/TSX |
| `tooManyArguments` | `require()` avec >1 arg | ✅ | ✗ | TS/JS/TSX |

### eslint-plugin-tree-shaking
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-side-effects-in-initialization` | Flag top-level side effects | ✅ | ≈ `no_constructor_side_effects`, `no_unassigned_import` (partial) | TS/JS/TSX |

### eslint-plugin-write-good-comments
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `write-good-comments` | Lint prose (weasel words, passive voice) — port btford/write-good | ✅ | ✓ `comment_prose_quality` | TS/JS/TSX/Rust/Vue |

### eslint-plugin-xstate
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `entry-exit-action` | Shape of `entry`/`exit` | ⚠️ | ✗ | TS/JS/TSX |
| `event-names` | Naming convention on events | ⚠️ | ✗ | TS/JS/TSX |
| `invoke-usage` | Validate `invoke` config | ⚠️ | ✗ | TS/JS/TSX |
| `no-async-guard` | Guards must not be async | ✅ | ✗ | TS/JS/TSX |
| `no-auto-forward` | Forbid `autoForward: true` | ⚠️ | ✗ | TS/JS/TSX |
| `no-imperative-action` | Forbid `send()` outside actions | ⚠️ | ✗ | TS/JS/TSX |
| `no-infinite-loop` | Detect always-transitions looping | ✅ | ✗ | TS/JS/TSX |
| `no-inline-implementation` | Require named actions/guards | ✅ | ✗ | TS/JS/TSX |
| `no-invalid-conditional-action` | Validate `cond`/`guard` shape | ✅ | ✗ | TS/JS/TSX |
| `no-invalid-state-props` | Only known state props | ✅ | ✗ | TS/JS/TSX |
| `no-invalid-transition-props` | Only known transition props | ✅ | ✗ | TS/JS/TSX |
| `no-misplaced-on-transition` | `on` on state nodes only | ✅ | ✗ | TS/JS/TSX |
| `no-ondone-outside-compound-state` | `onDone` on compound states | ✅ | ✗ | TS/JS/TSX |
| `prefer-always` | Prefer `always` over empty key | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-predictable-action-arguments` | Force flag | ⚠️ | ✗ | TS/JS/TSX |
| `spawn-usage` | `spawn()` inside `assign` | ✅ | ✗ | TS/JS/TSX |
| `state-names` | State naming convention | ⚠️ | ✗ | TS/JS/TSX |
| `system-id` | Require `systemId` | ⚠️ | ✗ | TS/JS/TSX |

**Note**: XState rules à implémenter comme pack dédié si demandé.

### eslint-plugin-zod
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `array-style` | `z.array(X)` vs `X.array()` | ⚠️ | ✗ | TS/JS/TSX |
| `consistent-import-source` | Single import source | ✅ | ✗ | TS/JS/TSX |
| `consistent-import` | `{ z }` vs `* as z` | ⚠️ | ≈ `no_namespace_import` (opposite) | — |
| `consistent-object-schema-type` | `z.object` vs `z.strictObject` | ⚠️ | ✗ | TS/JS/TSX |
| `consistent-schema-output-type-style` | `z.output` vs `z.infer` | ⚠️ | ✗ | TS/TSX |
| `no-empty-custom-schema` | Forbid `z.custom()` no validator | ✅ | ✗ | TS/JS/TSX |
| `no-number-schema-with-int` | Prefer `z.int()` | ✅ | ✗ | TS/JS/TSX |
| `no-optional-and-default-together` | Forbid `.optional().default()` | ✅ | ≈ `zod_no_optional_nullable_chain` (partial) | TS/JS/TSX |
| `no-string-schema-with-uuid` | Prefer `z.uuid()` | ✅ | ✗ | TS/JS/TSX |
| `no-throw-in-refine` | Forbid `throw` in refine | ✅ | ✗ | TS/JS/TSX |
| `no-transform-in-record-key` | Forbid transform in record key | ✅ | ✗ | TS/JS/TSX |
| `no-unknown-schema` | Forbid `z.unknown()` | ⚠️ | ≈ `zod_no_any` (any only) | TS/JS/TSX |
| `prefer-enum-over-literal-union` | Prefer `z.enum([...])` | ✅ | ✗ | TS/JS/TSX |
| `prefer-meta-last` | `.meta()` last in chain | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-meta` | Require `.meta()` on schemas | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-namespace-import` | Prefer `import * as z` | ⚠️ | ✗ (conflict) | — |
| `prefer-string-schema-with-trim` | Prefer `.trim()` | ⚠️ | ≈ `zod_trim_before_min` | TS/JS/TSX |
| `require-brand-type-parameter` | `.brand<Type>()` | ✅ | ≈ `zod_brand_ids` (IDs only) | TS/TSX |
| `require-schema-suffix` | Identifiers end with `Schema` | ⚠️ | ✗ | TS/JS/TSX |
| `schema-error-property-style` | `{ message }` vs `{ error }` | ⚠️ | ≈ `zod_require_error_messages` (presence only) | TS/JS/TSX |

**Améliorations zod existantes**:
- `zod_no_optional_nullable_chain` → inclure `.optional().default()`
- `zod_no_any` → option pour inclure `z.unknown()`
- `zod_brand_ids` → valider type param sur tout `.brand()`

### eslint-plugin-check-file
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `filename-blocklist` | Forbid filenames matching globs | ⚠️ | ✗ | All |
| `filename-naming-convention` | Glob→naming-case mapping | ✅ | ✗ | All |
| `folder-match-with-fex` | File under folder by extension | ⚠️ | ✗ | All |
| `folder-naming-convention` | Case conventions on folders | ✅ | ✗ | All |
| `no-index` | Forbid `index.*` | ⚠️ | ✗ (conflict barrel) | — |

---

## Import

### eslint-plugin-import-x
| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `default` | Import default doit exister | ✅ | ✗ | TS/JS/TSX |
| `export` | Interdit exports dupliqués | ✅ | ≈ `no_duplicate_imports` (imports only) | TS/JS/TSX |
| `extensions` | Enforce/interdit extensions | ⚠️ | ✗ | TS/JS/TSX |
| `group-exports` | Fusionne exports nommés | ⚠️ | ✗ | TS/JS/TSX |
| `named` | Le binding nommé existe | ✅ | ✗ | TS/JS/TSX |
| `namespace` | Usage valide `import * as x` | ✅ | ✗ | TS/JS/TSX |
| `no-commonjs` | Interdit require/module.exports | ⚠️ | ≈ `import_no_commonjs` | TS/JS/TSX |
| `no-cycle` | Détection cycles | ✅ | ✗ | TS/JS/TSX/Rust |
| `no-deprecated` | Import de `@deprecated` | ⚠️ | ✗ | TS/JS/TSX |
| `no-extraneous-dependencies` | Import non listé package.json | ✅ | ✓ `no_implicit_deps` | TS/JS/TSX |
| `no-internal-modules` | Interdit deep imports | ✅ | ≈ `api_import_from_public_index`, `layer_import_boundary` | TS/JS/TSX/Rust |
| `no-named-as-default` | Import named = default elsewhere | ✅ | ✗ | TS/JS/TSX |
| `no-named-as-default-member` | `foo.bar` où bar est export | ✅ | ✗ | TS/JS/TSX |
| `no-nodejs-modules` | Import `fs`, `path` | ⚠️ | ✗ | TS/JS/TSX |
| `no-relative-packages` | `../../other-package/` | ✅ | ✗ | TS/JS/TSX |
| `no-relative-parent-imports` | `../...` | ⚠️ | ✗ | TS/JS/TSX |
| `no-rename-default` | `import Foo` si default = Bar | ✅ | ✗ | TS/JS/TSX |
| `no-restricted-paths` | Zones interdites | ⚠️ | ✗ | TS/JS/TSX |
| `no-unresolved` | Module introuvable | ⚠️ | ✗ | TS/JS/TSX |
| `no-unused-modules` | Export jamais consommé | ✅ | ✓ `dead_export` | TS/JS/TSX/Rust |
| `no-useless-path-segments` | `./foo/../bar` | ✅ | ✗ | TS/JS/TSX |
| `order` | Ordre de groupes | ⚠️ | ✗ | TS/JS/TSX |
| `prefer-default-export` | Single export → default | ⚠️ | ✗ | TS/JS/TSX |
| `unambiguous` | File ESM/script unambiguous | ✅ | ✗ | TS/JS |

**Haute priorité import**: `no-cycle` (cross-file via ImportIndex).

---

## TypeScript-ESLint

| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `array-type` | `T[]` vs `Array<T>` | ⚠️ | ✓ oxlint delegated | TS/TSX |
| `no-duplicate-type-constituents` | Interdit `A \| A` | ✅ | ✓ `no_duplicate_in_composite` | TS/TSX |
| `no-empty-interface` | Interdit interface vide | ✅ | ✓ `ts_no_empty_object_type` | TS/TSX |
| `no-explicit-any` | Interdit `any` | ✅ | ✓ oxlint delegated | TS/TSX |
| `no-non-null-assertion` | Interdit `x!` | ⚠️ | ✓ oxlint delegated | TS/TSX |
| `no-require-imports` | Interdit `require('x')` | ✅ | ✓ oxlint delegated | TS/TSX |
| `no-type-alias` | Restrictions sur type aliases | ⚠️ | ✗ (deprecated) | — |
| `no-unsafe-function-type` | Interdit `Function` type | ✅ | ✓ oxlint delegated | TS/TSX |
| `no-var-requires` | Interdit `const x = require` | ✅ | ✓ oxlint delegated | TS/TSX |
| `prefer-as-const` | Préfère `as const` | ✅ | ✓ oxlint delegated | TS/TSX |
| `typedef` | Annotations requises | ⚠️ | ✗ (deprecated) | — |

**Note**: Quasi-totalité déjà couverte via oxlint delegation.

---

## HTML-ESLint

| Rule | Description | Rec | comply | Scope |
|------|-------------|-----|--------|-------|
| `no-abstract-roles` | `role="widget"` abstraits | ✅ | ✓ `a11y_no_abstract_roles` | HTML/Vue |
| `no-accesskey-attrs` | `accesskey=""` interdit | ✅ | ✓ `a11y_no_accesskey` | HTML/Vue |
| `no-aria-hidden-body` | `<body aria-hidden>` | ✅ | ✓ `a11y_no_aria_hidden_on_body` | HTML/Vue |
| `no-aria-hidden-on-focusable` | focusable + aria-hidden | ✅ | ✓ `a11y_aria_hidden_no_focusable` | HTML/Vue |
| `no-duplicate-attrs` | Attribut dupliqué | ✅ | ✗ | HTML/Vue |
| `no-duplicate-id` | `id="x"` en double | ✅ | ✗ | HTML/Vue |
| `no-duplicate-in-head` | title/meta doublés | ✅ | ✗ | HTML/Vue |
| `no-empty-headings` | `<h1></h1>` vide | ✅ | ✓ `a11y_no_empty_heading` | HTML/Vue |
| `no-heading-inside-button` | `<button><h1>` | ✅ | ✓ `a11y_no_heading_inside_button` | HTML/Vue |
| `no-inline-styles` | `style=""` interdit | ⚠️ | ✗ | HTML/Vue |
| `no-invalid-role` | `role="notARole"` | ✅ | ✓ `a11y_no_invalid_role` | HTML/Vue |
| `no-nested-interactive` | `<a><button>` | ✅ | ✓ `a11y_no_nested_interactive` | HTML/Vue |
| `no-obsolete-tags` | `<center>`, `align=` | ✅ | ✗ | HTML/Vue |
| `no-positive-tabindex` | `tabindex > 0` | ✅ | ✓ `a11y_no_positive_tabindex` | HTML/Vue |
| `no-redundant-role` | `<button role="button">` | ✅ | ✓ `a11y_no_redundant_role` | HTML/Vue |
| `no-skip-heading-levels` | h1→h3 interdit | ✅ | ✓ `a11y_no_skip_heading_levels` | HTML/Vue |
| `no-target-blank` | `target="_blank"` sans noopener | ✅ | ✓ `a11y_no_target_blank` | HTML/Vue |
| `no-script-style-type` | `<script type>` inutile | ✅ | ✗ | HTML/Vue |
| `prefer-https` | `http://` dans href/src | ✅ | ✗ | HTML/Vue |
| `require-button-type` | `<button type>` | ✅ | ✓ `a11y_require_button_type` | HTML/Vue |
| `require-closing-tags` | `<div>` non fermé | ✅ | ✗ | HTML/Vue |
| `require-doctype` | `<!doctype html>` | ✅ | ✗ | HTML |
| `require-img-alt` | `<img alt>` | ✅ | ✓ `a11y_require_img_alt` | HTML/Vue |
| `require-lang` | `<html lang>` | ✅ | ✓ `a11y_require_html_lang` | HTML/Vue |
| `require-meta-charset` | `<meta charset>` | ✅ | ✗ | HTML |
| `require-title` | `<title>` | ✅ | ✗ | HTML |
| `require-frame-title` | `<iframe title>` | ✅ | ✓ `a11y_require_iframe_title` | HTML/Vue |
| `no-non-scalable-viewport` | `user-scalable=no` | ✅ | ✗ | HTML/Vue |

**Note**: ~60% couvert via règles `a11y_*`.

---

## Résumé statistique

| Catégorie | ✅ Recommandé | ⚠️ Optionnel | Déjà comply |
|-----------|--------------|--------------|-------------|
| Security | 13 | 10 | ~6 (partiel) |
| React | 6 | 11 | ~2 |
| Functional | 4 | 21 | ~8 |
| Architecture | 7 | 1 | ~2 (partiel) |
| i18n | 1 | 4 | ~1 (partiel) |
| Testing (vitest) | 35 | 15 | ~12 (via playwright_*) |
| Testing (playwright) | 12 | 5 | ~6 |
| Tailwind | 8 | 6 | ~2 (partiel) |
| Utility | 14 | 27 | ~8 |
| Import | 16 | 10 | ~4 |
| TypeScript | 8 | 3 | **~8 (tous via oxlint)** |
| HTML | 27 | 1 | ~14 (via a11y_*) |
| XState | 10 | 8 | 0 |
| Zod | 10 | 10 | ~5 (partiel) |
| Misc | 7 | 3 | ~3 |
| **Total** | **~178** | **~135** | **~80** |

---

## Top priorities (impact maximal, effort minimal)

### Nouvelles règles haute valeur
1. **no-unsanitized/method + property** — XSS sinks, TS/JS/TSX/Vue
2. **react-perf** — 4 règles `jsx-no-new-*-as-prop`
3. **listeners** — 3 règles memory leaks DOM
4. **no-cycle** (import-x) — cycles via ImportIndex
5. **barrel-files** — 3 règles tree-shaking
6. **no-empty-catch** — catch vide, TS/JS/TSX/Rust
7. **no-electron-node-integration** — sécurité Electron
8. **no-assign-mutated-array** — FP, `const x = arr.sort()`

### Extensions de règles existantes
1. `no-weak-ssl` → `rejectUnauthorized: false` + `NODE_TLS_REJECT_UNAUTHORIZED`
2. `no-eval` → `vm.runInThisContext/runInContext/runInNewContext`
3. `no-hardcoded-secret` → Shannon entropy scan
4. `tailwind_prefer_size_shorthand` → tous les shorthands
5. `playwright_*` → généraliser en règles testing génériques
6. `no_duplicate_imports` → dedupe named specifiers
7. `ts_no_restricted_imports` → presets `module-replacements`
