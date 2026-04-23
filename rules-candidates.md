# Règles candidates pour comply

Analyse de 84 plugins ESLint. 924 règles existantes vérifiées pour éviter les doublons.

---

## Haute priorité (HIGH)

### Sécurité

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `no-postmessage-star-origin` | Interdit `postMessage(msg, '*')` — fuite cross-origin | sdl | security |
| `no-document-domain` | Interdit `document.domain =` — affaiblit same-origin | sdl | security |
| `no-document-write` | Interdit `document.write` — XSS + perf | sdl | security |
| `no-inner-html` | Interdit `innerHTML/outerHTML =` — XSS | sdl | security |
| `no-insecure-url` | Flag `http://` URLs dans le code | sdl | security |
| `no-unsafe-alloc` | Interdit `Buffer.allocUnsafe` / `new Buffer(size)` | sdl | security |
| `detect-child-process` | Détecte `child_process.exec` avec input non-literal | security-node | security |
| `detect-dangerous-redirects` | Détecte `res.redirect(userInput)` | security-node | security |
| `detect-eval-with-expr` | Détecte `eval()` avec expression non-literal | security-node | security |
| `detect-non-literal-require` | Interdit `require(variable)` | security-node | security |
| `detect-option-rejectunauthorized` | Flag `rejectUnauthorized: false` | security-node | security |
| `no-unsafe-regex` | Détecte regex susceptibles à ReDoS | redos-detector | security |
| `react-no-javascript-urls` | Interdit `href="javascript:..."` en JSX | react-security | security |
| `html-no-target-blank` | Require `rel="noopener"` sur `target="_blank"` (HTML) | html-eslint | security |

### Bugs / Correctness

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `no-floating-promise` | Flag Promises non-awaited/non-catchées | ai-guard / no-floating-promise | async |
| `no-async-without-await` | Flag `async` functions sans `await` | ai-guard | async |
| `no-async-array-callback` | Interdit callbacks async dans `map/forEach/filter` | ai-guard | async |
| `valid-event-listener` | `addEventListener/removeEventListener` avec même référence | fsecond / listeners | correctness |
| `no-missing-remove-event-listener` | Require `removeEventListener` pour chaque `addEventListener` | listeners | correctness |
| `throw-error-values` | Interdit `throw 'string'` / `throw {}` — utiliser Error | etc | code-quality |
| `exception-use-error-cause` | Require `{ cause: originalErr }` dans re-throw | exception-handling | code-quality |
| `try-catch-json-parse` | Require try/catch autour de `JSON.parse` | try-catch-failsafe | error-handling |
| `try-catch-new-url` | Require try/catch autour de `new URL()` | try-catch-failsafe | error-handling |
| `no-one-iteration-loop` | Flag boucles qui ne peuvent itérer qu'une fois | radar | bugs |
| `no-extra-arguments` | Flag appels avec plus d'args que de params | radar | bugs |
| `no-use-of-empty-return-value` | Flag usage du retour d'une fonction void | radar | bugs |
| `no-submit-handler-without-preventDefault` | `onSubmit` doit appeler `preventDefault()` | upleveled | react |
| `ts-only-throw-error` | Interdit throw de non-Error (détectable par AST) | typescript-eslint | bugs |
| `ts-prefer-promise-reject-errors` | Interdit `Promise.reject(non-Error)` | typescript-eslint | bugs |
| `block-scope-case` | Require `{}` dans case avec déclarations | blitz | code-quality |

### Imports / Architecture

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `import-no-cycle` | Interdit les imports cycliques | import-x | imports |
| `import-no-extraneous-dependencies` | Interdit imports de packages non listés dans package.json | import-x | imports |
| `import-no-restricted-paths` | Enforce boundaries entre layers (domain ⇏ infra) | import-x | architecture |
| `avoid-importing-barrel-files` | Warn quand on importe d'un barrel file | barrel-files | imports |
| `import-dedupe` | Flag imports dupliqués du même module | antfu | imports |
| `no-full-import` | Interdit `import _ from 'lodash'` — require subpath | small-import | imports |
| `no-test-imports-in-prod` | Interdit imports de fichiers test dans prod | fast-import | imports |
| `require-path-exists` | Valide que les chemins import/require existent | require-path-exists | imports |

### React / Performance

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `jsx-no-new-function-as-prop` | Interdit fonctions inline comme props JSX | react-perf | react |
| `jsx-ensure-booleans` | Require conversion boolean dans `{cond && <X/>}` | jsx-conditionals | react |
| `react-no-adjust-state-on-prop-change` | Interdit useEffect qui set state sur prop change | react-you-might-not-need-an-effect | react |
| `react-no-pass-data-to-parent` | Interdit useEffect qui appelle callback parent | react-you-might-not-need-an-effect | react |
| `react-no-reset-all-state-on-prop-change` | Utiliser key plutôt que reset useEffect | react-you-might-not-need-an-effect | react |
| `react-no-chain-state-updates` | Interdit setState cascadés dans un effect | react-you-might-not-need-an-effect | react |
| `require-size-attributes` | Require width/height sur img/iframe/video (CLS) | layout-shift / html-eslint | performance |
| `react-hook-form-destructuring-formstate` | Require destructuration de formState | react-hook-form | react |

### Database

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `enforce-delete-with-where` | Require `.where()` sur Drizzle `.delete()` | drizzle | drizzle |
| `enforce-update-with-where` | Require `.where()` sur Drizzle `.update()` | drizzle | drizzle |
| `pg-require-limit` | Require LIMIT sur SELECT PostgreSQL | postgresql | database |

### Testing

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `playwright-missing-playwright-await` | Require await sur APIs async Playwright | playwright | testing |
| `playwright-expect-expect` | Require au moins un expect() par test | playwright | testing |
| `playwright-no-eval` | Interdit `page.$eval` — prefer locators | playwright | testing |
| `playwright-prefer-locator` | Prefer `page.locator()` over `page.$` | playwright | testing |
| `vitest-hoisted-apis-on-top` | `vi.mock` doit être avant les imports | vitest | testing |
| `vitest-no-disabled-tests` | Flag tests skip/disabled | vitest | testing |

### TypeScript

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `ts-consistent-type-imports` | Enforce `import type` pour type-only | typescript-eslint | typescript |
| `ts-consistent-type-exports` | Enforce `export type` pour type-only | typescript-eslint | typescript |
| `ts-no-non-null-assertion` | Interdit `!` non-null assertion | typescript-eslint | typescript |

### i18n

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `i18n-json-identical-keys` | Vérifier que tous les locales ont les mêmes clés | i18n-json | i18n |
| `i18n-json-identical-placeholders` | Vérifier que les {{placeholders}} matchent | i18n-json | i18n |
| `i18n-json-valid-message-syntax` | Valider syntaxe ICU dans fichiers locale | i18n-json | i18n |

### HTML / A11y

| ID | Description | Source | Catégorie |
|----|-------------|--------|-----------|
| `html-no-abstract-roles` | Interdit ARIA abstract roles | html-eslint | a11y |
| `html-no-aria-hidden-body` | Interdit `aria-hidden` sur `<body>` | html-eslint | a11y |
| `html-no-nested-interactive` | Interdit éléments interactifs imbriqués | html-eslint | a11y |
| `html-no-skip-heading-levels` | Interdit sauter niveaux heading (h1→h3) | html-eslint | a11y |
| `html-no-positive-tabindex` | Interdit `tabindex > 0` | html-eslint | a11y |
| `html-no-invalid-attr-value` | Valide valeurs d'attributs énumérés | html-eslint | html |
| `html-require-button-type` | Require `type` explicite sur `<button>` | html-eslint | html |
| `html-require-img-alt` | Require `alt` sur `<img>` (HTML pur) | html-eslint | a11y |
| `html-require-input-label` | Require `<label>` pour `<input>` | html-eslint | a11y |
| `html-require-explicit-size` | Require width/height (CLS) en HTML | html-eslint | perf |

---

## Moyenne priorité (MEDIUM)

### Code Quality

| ID | Description | Source |
|----|-------------|--------|
| `prefer-early-return` | Prefer guard clauses / early return | prefer-early-return |
| `no-mutation` | Interdit mutations sur `const` bindings | const-immutable |
| `no-mutating-methods` | Interdit `.push/.pop/.sort/.reverse/.splice` | fp |
| `functional-immutable-data` | Interdit mutation de params et variables | functional |
| `de-morgan-simplify` (extend) | Vérifier couverture negated-conjunction | de-morgan |
| `prefer-single-boolean-return` | `if(x) return true; return false` → `return x` | radar |
| `no-catch-log-rethrow` | Interdit catch qui log + rethrow seulement | ai-guard |
| `no-catch-without-use` | Require que l'erreur catchée soit utilisée | ai-guard |
| `proper-arrows-return` | Interdit concise-body confus (objects, ternaries) | proper-arrows |
| `visual-complexity` | Complexité visuelle (variante cognitive) | visual-complexity |

### Tailwind

| ID | Description | Source |
|----|-------------|--------|
| `enforce-shorthand-classes` | Collapse `h-4 w-4` → `size-4` | better-tailwindcss |
| `tailwind-no-deprecated-classes` | Flag classes Tailwind v2/v3 dépréciées | better-tailwindcss |
| `enforce-logical-properties` | Prefer `ms-`/`me-` over `ml-`/`mr-` (RTL) | better-tailwindcss |
| `tailwind-classnames-order` | Ordre canonique des classes | tailwindcss |
| `tailwind-no-custom-classname` | Warn sur classes inconnues | tailwindcss |

### Node / Package.json

| ID | Description | Source |
|----|-------------|--------|
| `pkg-no-dupe-deps` | Interdit package dans dependencies ET devDependencies | node-dependencies |
| `pkg-valid-semver` | Valide que les versions sont semver valides | node-dependencies |
| `pkg-absolute-version` | Interdit `^`/`~` pour certains deps | node-dependencies |

### Architecture

| ID | Description | Source |
|----|-------------|--------|
| `boundaries-element-types` | Enforce quels types peuvent importer quoi | boundaries |
| `boundaries-external` | Restrict packages externes par layer | boundaries |
| `boundaries-entry-point` | Import via entry point seulement | boundaries |
| `index-only-import-export` | index.ts ne doit contenir que imports/exports | index |

### Zod

| ID | Description | Source |
|----|-------------|--------|
| `zod-no-optional-and-default-together` | `.optional().default()` est redondant | zod |
| `zod-no-unknown-schema` | Interdit `z.unknown()` sauf whitelist | zod |
| `zod-require-schema-suffix` | Require suffix `...Schema` sur exports | zod |

### XState

| ID | Description | Source |
|----|-------------|--------|
| `xstate-entry-exit-action` | Valide shape des entry/exit actions | xstate |
| `xstate-invoke-usage` | Valide structure des invoke configs | xstate |
| `xstate-no-imperative-action` | Interdit send/raise impératifs | xstate |

### Vitest

| ID | Description | Source |
|----|-------------|--------|
| `vitest-no-duplicate-hooks` | Flag duplicate beforeEach/afterEach | vitest |
| `vitest-no-large-snapshots` | Flag snapshots > N lignes | vitest |
| `vitest-require-test-timeout` | Require timeout sur tests async longs | vitest |
| `vitest-prefer-mock-return-shorthand` | Prefer `.mockReturnValue` over `.mockImplementation` | vitest |

---

## Basse priorité (LOW) — Opt-in / Stylistic

- `no-null` — Prefer undefined over null
- `no-let` — FP style: const only
- `no-delete` — Prefer rest-spread
- `no-foreach` — Prefer for...of
- `no-index-file` — Forbid index.ts files
- `filename-blocklist` — Forbid patterns like `*.util.ts`
- `top-level-function` — Prefer `function` over `const fn = () =>`
- `clsx-*` — Various clsx/cn preferences
- `proper-arrows-*` — Arrow function constraints (mostly stylistic)
- `toml-*` — TOML formatting rules
- `markdown-*` — Markdown formatting rules
- `vitest-prefer-lowercase-title` — Test naming conventions
- `xstate-event-names` / `xstate-state-names` — Naming conventions

---

## Règles exclues

### Déjà dans comply (duplicates vérifiés)
- `avoid-barrel-files`, `avoid-re-export-all`
- `de-morgan-simplify`
- `no-commented-out-code`
- `no-duplicate-imports`, `no-useless-path-segments`
- `no-hardcoded-secret`
- `jsx-no-leaked-render`
- `drizzle-no-sql-raw-with-variable`
- `cognitive-complexity`, `no-identical-functions`
- `ts-no-const-enum`
- `tanstack-query-array-key`
- `no-focused-test`, `no-eval`
- `a11y-*` (33 rules JSX)
- `react-no-find-dom-node`, `react-no-dangerously-set-inner-html`
- `no-unsanitized-method/property`
- Playwright: 25+ rules already covered

### Nécessitent type info TypeScript
- `no-floating-promises` (ts-eslint version)
- `no-misused-promises`
- `no-unnecessary-condition`
- `strict-boolean-expressions`
- `restrict-template-expressions`
- `await-thenable`
- `no-unsafe-*` family
- `prefer-nullish-coalescing` (full version)
- `functional-no-return-void`

### Trop spécifiques / Frameworks rares
- `eslint-plugin-es-x` (200+ compat rules — need browserslist engine)
- Rules RHF very specific (except destructuring-formstate)
- `project-structure-*` (config-heavy)

---

## Statistiques

- **Plugins analysés:** 84
- **Règles existantes comply:** 924
- **Candidats HIGH:** ~65 règles
- **Candidats MEDIUM:** ~45 règles
- **Candidats LOW:** ~25 règles
- **Doublons évités:** ~80 règles
- **Exclues (type info):** ~30 règles
