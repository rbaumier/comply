# TODO — Rules Backlog Implementation

Toutes les règles à implémenter issues de l'analyse des 52 plugins ESLint.
Consolidé le 2026-04-23.

---

## Extensions de règles existantes

### Security
- [ ] `no-weak-ssl` → ajouter détection `rejectUnauthorized: false` dans `https.request()` et `new Agent()`
- [ ] `no-weak-ssl` → ajouter détection `process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'`
- [ ] `no-eval` → ajouter `vm.runInThisContext`, `vm.runInContext`, `vm.runInNewContext` avec arg non-literal
- [ ] `no-eval` → ajouter `new Function(nonLiteral)`
- [ ] `no-hardcoded-secret` → ajouter Shannon entropy scan (>4.5 bits/char sur strings >20 chars)
- [ ] `no-hardcoded-secret` → ajouter patterns GCP `"type": "service_account"`, Facebook OAuth, Twitter OAuth, Heroku UUID

### React
- [ ] `react-jsx-no-bind` → renommer/étendre en `react-jsx-no-new-function-as-prop` (inclure arrows, function expressions, pas seulement `.bind()`)

### Testing
- [ ] Généraliser `playwright_no_duplicate_hooks` → `no_duplicate_hooks` (jest/vitest/playwright)
- [ ] Généraliser `playwright_no_hooks` → `no_hooks`
- [ ] Généraliser `playwright_prefer_hooks_in_order` → `prefer_hooks_in_order`
- [ ] Généraliser `playwright_prefer_hooks_on_top` → `prefer_hooks_on_top`
- [ ] Généraliser `playwright_prefer_comparison_matcher` → `prefer_comparison_matcher`
- [ ] Généraliser `playwright_prefer_equality_matcher` → `prefer_equality_matcher`
- [ ] Généraliser `playwright_prefer_strict_equal` → `prefer_strict_equal`
- [ ] Généraliser `playwright_prefer_to_be` → `prefer_to_be`
- [ ] Généraliser `playwright_prefer_to_contain` → `prefer_to_contain`
- [ ] Généraliser `playwright_max_expects` → `max_expects`
- [ ] `playwright_no_standalone_expect` → étendre en `valid_expect` (check args, `.resolves`/`.rejects` awaited)
- [ ] `testing_no_and_in_test_name` → étendre en `valid_test_title` (non-empty, non-duplicate-prefix, pas espaces)
- [ ] `playwright_no_element_handle` → ajouter `page.$eval` / `page.$$eval`

### Tailwind
- [ ] `tailwind_prefer_size_shorthand` → généraliser tous les shorthands (padding, margin, rounded, border, inset, scroll-*)

### FP
- [ ] `no_array_sort_mutation` → étendre pour inclure `push`, `pop`, `shift`, `unshift`, `splice`, `copyWithin`, `fill`, `reverse`
- [ ] `no_null` → option pour inclure `undefined` literal (subsume `no-nil`)

### Import
- [ ] `no_duplicate_imports` → détecter aussi duplicate named specifiers dans une déclaration `import { a, a }`
- [ ] `no_duplicate_imports` → détecter aussi exports dupliqués
- [ ] `ts_no_restricted_imports` → ajouter `presets` config key (`microutilities`, `native`, `preferred`) avec table `module-replacements`

### Zod
- [ ] `zod_no_optional_nullable_chain` → inclure `.optional().default(...)` chain
- [ ] `zod_no_any` → option pour inclure `z.unknown()`
- [ ] `zod_brand_ids` → valider présence type param sur tout `.brand()`, pas seulement IDs
- [ ] `zod_require_error_messages` → option pour enforcer style `{ message }` vs `{ error }`

### i18n
- [ ] `i18n_no_hardcoded_string_in_jsx` → étendre hors JSX (tous les string literals user-facing)

### Listeners
- [ ] `no_invalid_remove_event_listener` → valider aussi que add/remove utilisent même handler

### Cookie/Session (généralisation)
- [ ] `hono_cookie_no_secure` → généraliser pour Express `res.cookie()` et Fastify

---

## Nouvelles règles — Security

- [ ] `no-unsanitized-method` — Flag `insertAdjacentHTML`, `document.write`, `document.writeln`, `.setHTMLUnsafe`, `Range.createContextualFragment` avec arg non-literal | TS/JS/TSX/Vue
- [ ] `no-unsanitized-property` — Flag assignment à `innerHTML`, `outerHTML`, `srcdoc` avec RHS non-literal | TS/JS/TSX/Vue
- [ ] `no-electron-node-integration` — Flag `nodeIntegration: true`, `nodeIntegrationInWorker: true`, `nodeIntegrationInSubFrames: true` dans `new BrowserWindow()` | TS/JS/TSX
- [ ] `react-no-find-dom-node` — Flag `ReactDOM.findDOMNode()` (deprecated React 19) | TS/JS/TSX
- [ ] `no-empty-catch` — Flag `catch (e) {}` avec body vide | TS/JS/TSX/Rust
- [ ] `express-session-require-name` — Flag `session({...})` sans `name` key | TS/JS
- [ ] `express-cookie-require-secure` — Flag `res.cookie(name, value, {httpOnly: false, secure: false})` | TS/JS
- [ ] `mysql-no-multiple-statements` — Flag `mysql.createConnection({multipleStatements: true})` | TS/JS
- [ ] `serialize-javascript-no-unsafe` — Flag `serialize(x, {unsafe: true})` | TS/JS
- [ ] `no-hardcoded-email` — Regex email dans strings/comments avec allowlist `example.com` | TS/JS/TSX/Vue/Rust

---

## Nouvelles règles — React

- [ ] `react-jsx-no-jsx-as-prop` — Flag JSX elements/fragments passés comme prop | TSX
- [ ] `react-jsx-no-new-array-as-prop` — Flag array literals inline comme prop | TSX
- [ ] `react-jsx-no-new-object-as-prop` — Flag object literals inline comme prop | TSX
- [ ] `react-no-empty-effect` — Flag `useEffect` avec body vide | TSX
- [ ] `react-no-initialize-state-in-effect` — Flag `setState` dans `useEffect` avec deps vides | TSX

---

## Nouvelles règles — FP

- [ ] `no-mutating-assign` — Flag `Object.assign(target, ...)` où target est non-empty object | TS/JS/TSX
- [ ] `no-valueof-field` — Flag définition de `valueOf` property/method | TS/JS/TSX
- [ ] `ts-no-mixed-types` — Flag type/interface mélangeant property signatures et method signatures | TS/TSX
- [ ] `ts-prefer-property-signatures` — Require `foo: () => T` over `foo(): T` method signatures | TS/TSX

---

## Nouvelles règles — Architecture

- [ ] `fsd-no-cross-slice-dependency` — Deux slices d'une même couche FSD ne peuvent pas s'importer | TS/JS/TSX/Vue
- [ ] `fsd-no-global-store-imports` — Interdit import store global depuis entities/shared/widgets | TS/JS/TSX/Vue
- [ ] `fsd-no-relative-imports` — Interdit imports relatifs traversant slices/couches | TS/JS/TSX/Vue
- [ ] `fsd-no-ui-in-business-logic` — Interdit `ui/` depuis `model/`, `api/`, `lib/` | TS/JS/TSX/Vue
- [ ] `avoid-barrel-files` — Flag fichier qui ne fait que ré-exporter (>= N `export *`/`export { x } from`) | TS/JS/TSX/Vue
- [ ] `avoid-importing-barrel-files` — Interdit importer depuis fichier détecté comme baril | TS/JS/TSX/Vue
- [ ] `avoid-re-export-all` — Interdit `export * from '...'` | TS/JS/TSX/Vue

---

## Nouvelles règles — Testing (vitest)

- [ ] `no-alias-methods` — Disallow Jest alias matchers (`toBeCalled` → `toHaveBeenCalled`) | TS/JS/TSX
- [ ] `no-conditional-tests` — Disallow conditionally-defined `test`/`describe` | TS/JS/TSX
- [ ] `no-done-callback` — Disallow `done`-style test callbacks | TS/JS/TSX
- [ ] `no-identical-title` — Disallow duplicate `describe`/`test` titles in same scope | TS/JS/TSX
- [ ] `no-import-node-test` — Disallow importing `node:test` in Vitest files | TS/JS/TSX
- [ ] `no-interpolation-in-snapshots` — Disallow template interpolation in snapshot matchers | TS/JS/TSX
- [ ] `no-mocks-import` — Disallow importing from `__mocks__` directly | TS/JS/TSX
- [ ] `no-test-prefixes` — Disallow `ftest`, `fdescribe`, `xtest`, `xdescribe` prefix forms | TS/JS/TSX
- [ ] `no-test-return-statement` — Disallow `return` in test body | TS/JS/TSX
- [ ] `prefer-called-exactly-once-with` — Prefer `toHaveBeenCalledExactlyOnceWith` | TS/JS/TSX
- [ ] `prefer-called-with` — Prefer `toHaveBeenCalledWith` over `toHaveBeenCalled` | TS/JS/TSX
- [ ] `prefer-expect-resolves` — Prefer `expect(promise).resolves` over `expect(await promise)` | TS/JS/TSX
- [ ] `prefer-mock-promise-shorthand` — Prefer `.mockResolvedValue`/`.mockRejectedValue` | TS/JS/TSX
- [ ] `prefer-mock-return-shorthand` — Prefer `.mockReturnValue` over `.mockImplementation(() => x)` | TS/JS/TSX
- [ ] `prefer-spy-on` — Prefer `vi.spyOn` over reassigning methods to `vi.fn()` | TS/JS/TSX
- [ ] `prefer-to-have-length` — Prefer `.toHaveLength` over `.length` + `.toBe` | TS/JS/TSX
- [ ] `prefer-todo` — Prefer `test.todo` over empty tests | TS/JS/TSX
- [ ] `require-hook` — Require side-effects inside a hook | TS/JS/TSX
- [ ] `require-to-throw-message` — Require message arg on `.toThrow()` | TS/JS/TSX
- [ ] `valid-describe-callback` — `describe` must take non-async function with no return | TS/JS/TSX
- [ ] `consistent-each-for` — Prefer `.each(...)` over loops/duplicated tests | TS/JS/TSX

---

## Nouvelles règles — Testing (playwright)

- [ ] `playwright-no-duplicate-slow` — Disallow duplicate `test.slow()` in same test | TS/JS
- [ ] `playwright-no-unused-locators` — Disallow locators created but never used | TS/JS
- [ ] `playwright-no-wait-for-timeout` — Disallow `page.waitForTimeout` | TS/JS
- [ ] `playwright-prefer-to-have-count` — Prefer `.toHaveCount(n)` over `.count()` + `.toBe()` | TS/JS
- [ ] `playwright-require-to-pass-timeout` — Require timeout on `expect(...).toPass()` | TS/JS
- [ ] `playwright-valid-test-tags` — Validate `{ tag: [...] }` option shape | TS/JS

---

## Nouvelles règles — Tailwind

- [ ] `tailwind-enforces-negative-arbitrary-values` — Flag `-top-[1px]`, suggest `top-[-1px]` | TS/JS/TSX/Vue/HTML
- [ ] `tailwind-migration-from-tailwind-2` — Detect obsolete v2 class names, suggest v3 | TS/JS/TSX/Vue/HTML
- [ ] `tailwind-no-deprecated-classes` — Flag v4-deprecated utilities | TS/JS/TSX/Vue/HTML
- [ ] `tailwind-consistent-variable-syntax` — Enforce `(--var)` vs `[--var]` CSS-variable syntax | TS/JS/TSX/Vue/HTML
- [ ] `tailwind-consistent-variant-order` — Enforce variant order in `md:hover:focus:...` | TS/JS/TSX/Vue/HTML
- [ ] `tailwind-no-unnecessary-whitespace` — Collapse extra spaces in class strings | TS/JS/TSX/Vue/HTML

---

## Nouvelles règles — Utility

- [ ] `no-import-dist` — Forbid importing from `dist/` directories | TS/JS/TSX
- [ ] `no-import-node-modules-by-path` — Forbid importing from `/node_modules/` by literal path | TS/JS/TSX
- [ ] `ts-no-export-equal` — Forbid `export = ...` in TS modules | TS/TSX
- [ ] `no-redundant-clsx` — Forbid `clsx("single-string")` or `cn("single-string")` | TS/JS/TSX
- [ ] `no-assign-mutated-array` — Forbid `const x = arr.sort()` / `arr.reverse()` / `arr.fill()` | TS/JS/TSX
- [ ] `ts-no-const-enum` — Forbid `const enum` declarations | TS/TSX
- [ ] `ts-no-implicit-any-catch` — Require explicit `: unknown` on `catch (e)` | TS/TSX
- [ ] `prefer-less-than` — Prefer `a < b` over `b > a` | TS/JS/TSX/Rust

---

## Nouvelles règles — Listeners

- [ ] `no-missing-remove-event-listener` — Chaque `addEventListener` doit avoir un `removeEventListener` | TS/JS/TSX/Vue
- [ ] `no-inline-function-event-listener` — Interdit `addEventListener('x', () => ...)` avec fonction inline | TS/JS/TSX/Vue

---

## Nouvelles règles — Arrows

- [ ] `arrow-this-in-function` — `this` dans arrow doit être imbriquée dans `function` | TS/JS/TSX

---

## Nouvelles règles — Import

- [ ] `no-cycle` — Détection cycles d'imports via ImportIndex (DFS/SCC) | TS/JS/TSX/Rust
- [ ] `no-named-as-default` — Flag `import foo from 'x'` quand `foo` est aussi export nommé dans `x` | TS/JS/TSX
- [ ] `no-named-as-default-member` — Flag `foo.bar` où `foo` est default import et `bar` est export nommé | TS/JS/TSX
- [ ] `no-relative-packages` — Interdit `../../other-package/` dans monorepo | TS/JS/TSX
- [ ] `no-rename-default` — Flag `import Foo from 'x'` si default export s'appelle `Bar` | TS/JS/TSX
- [ ] `no-useless-path-segments` — Flag `./foo/../bar` | TS/JS/TSX
- [ ] `import-default-exists` — Validate that default import exists in target module | TS/JS/TSX
- [ ] `import-named-exists` — Validate that named import binding exists in target's exports | TS/JS/TSX
- [ ] `import-namespace-usage` — Validate `foo.bar` where `foo` is `import * as foo` | TS/JS/TSX
- [ ] `require-not-empty` — Forbid empty string as import path | TS/JS/TSX
- [ ] `require-too-many-arguments` — `require()` called with more than one argument | TS/JS/TSX
- [ ] `unambiguous` — File must have import/export to be unambiguous ESM | TS/JS

---

## Nouvelles règles — Zod

- [ ] `zod-consistent-import-source` — Single import source (`zod` vs `zod/v4` vs `zod/mini`) | TS/JS/TSX
- [ ] `zod-no-empty-custom-schema` — Forbid `z.custom()` with no validator | TS/JS/TSX
- [ ] `zod-no-number-schema-with-int` — Prefer `z.int()` over `z.number().int()` | TS/JS/TSX
- [ ] `zod-no-string-schema-with-uuid` — Prefer `z.uuid()` over `z.string().uuid()` | TS/JS/TSX
- [ ] `zod-no-throw-in-refine` — Forbid `throw` inside `.refine`/`.superRefine` callbacks | TS/JS/TSX
- [ ] `zod-no-transform-in-record-key` — Forbid transforming schemas in `z.record` key position | TS/JS/TSX
- [ ] `zod-prefer-enum-over-literal-union` — Prefer `z.enum([...])` over `z.union([z.literal(...), ...])` | TS/JS/TSX

---

## Nouvelles règles — XState (pack dédié)

- [ ] `xstate-no-async-guard` — Guards must not be async / return promises | TS/JS/TSX
- [ ] `xstate-no-infinite-loop` — Detect always-transitions that loop forever | TS/JS/TSX
- [ ] `xstate-no-inline-implementation` — Forbid inline action/guard/service functions | TS/JS/TSX
- [ ] `xstate-no-invalid-conditional-action` — Validate `cond`/`guard` shape on transitions | TS/JS/TSX
- [ ] `xstate-no-invalid-state-props` — Only known XState state node props allowed | TS/JS/TSX
- [ ] `xstate-no-invalid-transition-props` — Only known transition props allowed | TS/JS/TSX
- [ ] `xstate-no-misplaced-on-transition` — `on` must live on state nodes | TS/JS/TSX
- [ ] `xstate-no-ondone-outside-compound-state` — `onDone` only on compound/invoking states | TS/JS/TSX
- [ ] `xstate-spawn-usage` — `spawn()` must be called inside `assign` | TS/JS/TSX

---

## Nouvelles règles — Check-file

- [ ] `filename-naming-convention` — Enforce glob→naming-case mapping on filenames | All
- [ ] `folder-naming-convention` — Enforce case conventions on folder segments | All

---

## Nouvelles règles — HTML

- [ ] `html-no-duplicate-attrs` — Attribut dupliqué sur même élément | HTML/Vue
- [ ] `html-no-duplicate-id` — `id="x"` en double dans le document | HTML/Vue
- [ ] `html-no-duplicate-in-head` — title/meta/charset doublés | HTML/Vue
- [ ] `html-no-obsolete-tags` — `<center>`, `<font>`, `align=` | HTML/Vue
- [ ] `html-no-script-style-type` — `<script type="text/javascript">` inutile | HTML/Vue
- [ ] `html-prefer-https` — `http://` dans `href`/`src` | HTML/Vue
- [ ] `html-require-closing-tags` — `<div>` non fermé | HTML/Vue
- [ ] `html-require-doctype` — `<!doctype html>` requis | HTML
- [ ] `html-require-meta-charset` — `<meta charset>` requis | HTML
- [ ] `html-require-title` — `<title>` requis | HTML
- [ ] `html-no-non-scalable-viewport` — `user-scalable=no` interdit | HTML/Vue

---

## Nouvelles règles — Misc

- [ ] `no-side-effects-in-initialization` — Flag top-level side effects (CallExpression, NewExpression, AssignmentExpression non-pure) sauf `/*#__PURE__*/` | TS/JS/TSX

---

## Statistiques

| Catégorie | Extensions | Nouvelles |
|-----------|------------|-----------|
| Security | 6 | 10 |
| React | 1 | 5 |
| FP | 2 | 4 |
| Architecture | 0 | 7 |
| Testing | 13 | 28 |
| Tailwind | 1 | 6 |
| Utility | 3 | 8 |
| Listeners | 1 | 2 |
| Import | 3 | 12 |
| Zod | 4 | 7 |
| XState | 0 | 9 |
| Check-file | 0 | 2 |
| HTML | 0 | 11 |
| Misc | 0 | 1 |
| **Total** | **34** | **112** |

**Grand total: 146 items**
