# Règles à implémenter dans comply

Document consolidé après review. 95 nouvelles règles à implémenter.

---

## Priorité 1 : Sécurité (12 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `no-unsafe-shell-exec` | Remplace `no-shell-exec`. Flag `child_process.exec()` seulement si l'argument contient du user input (variable, pas literal) | Medium |
| `no-postmessage-star-origin` | `postMessage(data, '*')` — fuite cross-origin | Easy |
| `no-document-domain` | `document.domain =` — affaiblit same-origin | Easy |
| `no-document-write` | `document.write()` — XSS + perf | Easy |
| `no-inner-html` | `innerHTML =` / `outerHTML =` — XSS (DOM vanilla, pas React) | Easy |
| `no-insecure-url` | URLs `http://` dans le code | Easy |
| `no-unsafe-alloc` | `Buffer.allocUnsafe()` / `new Buffer(size)` | Easy |
| `detect-dangerous-redirects` | `res.redirect(req.query.url)` — open redirect Express | Medium |
| `detect-option-rejectunauthorized` | `{ rejectUnauthorized: false }` — désactive TLS | Easy |
| `no-unsafe-regex` | Regex vulnérables ReDoS | Hard |
| `react-no-javascript-urls` | `href="javascript:..."` en JSX | Easy |

---

## Priorité 2 : Async / Promises (4 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `no-floating-promise` | Promise non-awaited/non-catchée. Heuristique : fonctions async connues (fetch, fs.promises, db.*) + fonctions async déclarées dans le fichier | Medium |
| `no-async-without-await` | Fonction `async` sans `await` | Easy |
| `no-async-array-callback` | `array.forEach(async ...)` — rejections perdues | Medium |
| `no-redundant-await` | `return await x` en fin de fonction async (sauf try/catch) | Easy |

---

## Priorité 3 : Error Handling (6 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `throw-error-values` | Interdit `throw 'string'` / `throw {}` | Easy |
| `exception-use-error-cause` | Re-throw doit utiliser `{ cause: originalErr }` | Medium |
| `try-catch-json-parse` | `JSON.parse()` doit être dans try/catch | Medium |
| `try-catch-new-url` | `new URL()` doit être dans try/catch | Medium |
| `no-catch-log-rethrow` | `catch(e) { log(e); throw e; }` — inutile | Medium |
| `no-catch-without-use` | `catch(e) { ... }` sans utiliser `e` | Easy |

---

## Priorité 4 : Imports (7 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `import-no-cycle` | Imports cycliques (utilise ImportIndex) | Hard |
| `import-no-extraneous-dependencies` | Import de package non listé dans package.json | Medium |
| `avoid-importing-barrel-files` | Warn quand on importe d'un barrel (consumer-side) | Medium |
| `import-dedupe` | `import { a, a } from 'x'` — doublon après merge | Easy |
| `no-full-import` | `import _ from 'lodash'` → require subpath | Easy |
| `no-test-imports-in-prod` | Import de fichiers test/mock dans code prod | Medium |
| `require-path-exists` | Valide que les chemins import/require existent | Medium |

---

## Priorité 5 : React (9 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `jsx-no-new-function-as-prop` | `onClick={() => ...}` inline (opt-in, severity hint) | Easy |
| `jsx-ensure-booleans` | `{items.length && <X/>}` doit être `{items.length > 0 && ...}` | Medium |
| `react-no-adjust-state-on-prop-change` | useEffect qui setState sur prop change | Hard |
| `react-no-pass-data-to-parent` | useEffect qui appelle callback parent | Hard |
| `react-no-reset-all-state-on-prop-change` | Reset state via useEffect → utiliser key | Hard |
| `react-no-chain-state-updates` | Multiple setState dans un effect | Medium |
| `react-hook-form-destructuring-formstate` | formState.isValid sans destructure | Easy |
| `no-submit-handler-without-preventDefault` | onSubmit sans preventDefault() | Easy |
| `react-hook-form-*` | Autres règles RHF principales | Medium |

---

## Priorité 6 : Database (3 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `enforce-delete-with-where` | `db.delete()` sans `.where()` | Easy |
| `enforce-update-with-where` | `db.update()` sans `.where()` | Easy |
| `pg-require-limit` | SELECT SQL sans LIMIT | Medium |

---

## Priorité 7 : Testing (4 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `playwright-missing-playwright-await` | Oubli de await sur page.click/expect | Medium |
| `playwright-expect-expect` | Test sans assertion | Easy |
| `playwright-no-eval` | `page.$eval()` legacy → prefer locator | Easy |
| `playwright-prefer-locator` | `page.$()` → `page.locator()` | Easy |
| `vitest-hoisted-apis-on-top` | `vi.mock()` doit être avant imports | Medium |
| `vitest-no-disabled-tests` | test.skip / xtest | Easy |
| `vitest-no-duplicate-hooks` | Duplicate beforeEach/afterEach | Easy |

---

## Priorité 8 : TypeScript (5 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `ts-consistent-type-imports` | Enforce `import type { X }` | Easy |
| `ts-consistent-type-exports` | Enforce `export type { X }` | Easy |
| `ts-no-non-null-assertion` | Interdit `value!` | Easy |
| `ts-only-throw-error` | throw de non-Error (literals) | Easy |
| `ts-prefer-promise-reject-errors` | `Promise.reject('msg')` → Error | Easy |

---

## Priorité 9 : HTML / A11y (10 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `html-no-abstract-roles` | ARIA abstract roles | Easy |
| `html-no-aria-hidden-body` | `aria-hidden` sur body | Easy |
| `html-no-nested-interactive` | `<button><a>` imbriqués | Medium |
| `html-no-skip-heading-levels` | h1 → h3 sans h2 | Easy |
| `html-no-positive-tabindex` | tabindex > 0 | Easy |
| `html-no-invalid-attr-value` | Valeurs d'attributs invalides | Medium |
| `html-require-button-type` | `<button>` sans type | Easy |
| `html-require-img-alt` | `<img>` sans alt (HTML pur) | Easy |
| `html-require-input-label` | `<input>` sans label | Medium |
| `html-require-explicit-size` | img/video sans width/height (CLS) | Easy |

---

## Priorité 10 : Performance (1 règle)

| ID | Description | Effort |
|----|-------------|--------|
| `require-size-attributes` | img/video sans width/height en JSX (CLS) | Easy |

---

## Priorité 11 : i18n (3 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `i18n-json-identical-keys` | Toutes les locales ont les mêmes clés | Hard |
| `i18n-json-identical-placeholders` | {{placeholders}} identiques | Hard |
| `i18n-json-valid-message-syntax` | Syntaxe ICU valide | Hard |

---

## Priorité 12 : Code Quality (7 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `no-one-iteration-loop` | Boucle qui ne peut itérer qu'une fois | Medium |
| `no-extra-arguments` | Appel avec plus d'args que de params | Medium |
| `no-use-of-empty-return-value` | Usage du retour d'une fonction void | Medium |
| `prefer-early-return` | Guard clauses over nested if | Medium |
| `block-scope-case` | case sans {} avec déclarations | Easy |
| `max-call-chain-depth` | **NOUVELLE** — Limite indirections `a() → b() → c() → d()` | Medium |
| `prefer-single-boolean-return` | `if(x) return true; return false` → `return x` | Easy |

---

## Priorité 13 : Immutabilité / FP (4 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `no-mutation` | Mutation sur `const` bindings | Medium |
| `no-mutating-methods` | `.push()/.pop()/.sort()/.reverse()/.splice()` | Easy |
| `functional-immutable-data` | Mutation de variables déclarées ailleurs | Hard |
| `no-delete` | `delete obj.prop` | Easy |
| `no-let` | Interdit `let` (prefer const) | Easy |

---

## Priorité 14 : Tailwind (3 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `tailwind-prefer-size-shorthand` (extend) | Étendre pour `px-2 py-2` → `p-2`, `mx-2 my-2` → `m-2`, etc. | Medium |
| `tailwind-no-deprecated-classes` | Classes v2/v3 dépréciées en v4 | Medium |
| `tailwind-classnames-order` | Ordre canonique des classes | Medium |

---

## Priorité 15 : Zod (3 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `zod-no-optional-and-default-together` | `.optional().default()` redondant | Easy |
| `zod-no-unknown-schema` | Interdit `z.unknown()` | Easy |
| `zod-require-schema-suffix` | Suffix `...Schema` sur exports | Easy |

---

## Priorité 16 : XState (5 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `xstate-entry-exit-action` | Valide structure entry/exit actions | Medium |
| `xstate-invoke-usage` | Valide configs invoke | Medium |
| `xstate-no-imperative-action` | Interdit send/raise hors action creators | Medium |
| `xstate-event-names` | Naming convention events (SCREAMING_SNAKE) | Easy |
| `xstate-state-names` | Naming convention states | Easy |

---

## Priorité 17 : Package.json (1 règle)

| ID | Description | Effort |
|----|-------------|--------|
| `pkg-no-dupe-deps` | Package dans dependencies ET devDependencies | Easy |

---

## Priorité 18 : Naming / Style (4 règles)

| ID | Description | Effort |
|----|-------------|--------|
| `no-index-file` | Interdit fichiers `index.ts` | Easy |
| `top-level-function` | Prefer `function` over `const fn = () =>` top-level | Easy |
| `proper-arrows-name` | Arrow functions assignées à variable nommée | Easy |
| `clsx-*` | Règles clsx/cn (à détailler) | Medium |

---

## Priorité 19 : TOML (nouveau backend)

| ID | Description | Effort |
|----|-------------|--------|
| `toml-no-mixed-type-in-array` | Arrays avec types mixtes | Medium |
| `toml-keys-order` | Ordre des clés | Easy |
| `toml-tables-order` | Ordre des [tables] | Easy |

---

## Priorité 20 : Markdown (nouveau backend)

| ID | Description | Effort |
|----|-------------|--------|
| `markdown-canonical-code-block-language` | Normalise `js` → `javascript` | Easy |
| `markdown-*` | Autres règles à détailler | Medium |

---

# Règles nécessitant type info

Ces règles nécessitent un type checker. **tsgo (TypeScript 7.0 Beta)** est disponible.

## Architecture : tsgolint

**Approche retenue : fork de [oxc-project/tsgolint](https://github.com/oxc-project/tsgolint)**

```
comply (Rust)
  └── tsgolint (binaire Go, subprocess)
        └── typescript-go internals (via go:linkname)
              └── Type checker natif (~10x plus rapide que tsc)
```

**Pourquoi tsgolint plutôt que LSP :**
- Accès direct au type checker (pas d'overhead IPC JSON-RPC)
- Pattern déjà utilisé par oxlint
- Plus performant pour le linting batch

**Comment ça marche (comme typescript-eslint) :**
```go
// Dans tsgolint, accès au type checker
checker := program.GetTypeChecker()
tsNode := mapper.GetTSNode(astNode)
nodeType := checker.GetTypeAtLocation(tsNode)

if tsutils.IsPromiseLike(checker, nodeType) {
    // C'est une Promise non-gérée !
}
```

## Règles type-aware à implémenter (en Go dans tsgolint)

| Règle | Description |
|-------|-------------|
| `no-floating-promises` | Promise non-awaited/catchée (version complète) |
| `no-misused-promises` | Promise dans un contexte qui attend void |
| `await-thenable` | await sur une valeur non-thenable |
| `strict-boolean-expressions` | Conditions non-boolean explicites |
| `no-unnecessary-condition` | Condition toujours true/false |

## Intégration dans comply

```bash
comply src/              # Mode rapide (tree-sitter, pas de types)
comply src/ --with-types # Mode complet (spawn tsgolint pour règles type-aware)
```

**Implémentation :**
1. Fork oxc-project/tsgolint
2. Ajouter les 5 règles type-aware en Go
3. comply spawn tsgolint comme subprocess (même pattern que oxlint/clippy)
4. tsgolint output JSON diagnostics, comply les merge avec les siens

## Dépendances

- Go 1.21+ (pour compiler tsgolint)
- typescript-go (submodule git dans tsgolint)
- Binaire tsgolint distribué avec comply (ou build à l'install)

---

# Résumé

| Catégorie | Nombre | Effort moyen |
|-----------|--------|--------------|
| Sécurité | 12 | Easy-Medium |
| Async | 4 | Medium |
| Error Handling | 6 | Medium |
| Imports | 7 | Medium |
| React | 9 | Medium-Hard |
| Database | 3 | Easy |
| Testing | 7 | Easy-Medium |
| TypeScript | 5 | Easy |
| HTML/A11y | 10 | Easy-Medium |
| Performance | 1 | Easy |
| i18n | 3 | Hard |
| Code Quality | 7 | Medium |
| FP/Immutabilité | 5 | Easy-Medium |
| Tailwind | 3 | Medium |
| Zod | 3 | Easy |
| XState | 5 | Medium |
| Package.json | 1 | Easy |
| Naming/Style | 4 | Easy |
| TOML | 3 | Medium |
| Markdown | 2+ | Easy-Medium |
| **TOTAL** | **~95** | |

---

# Règles à NE PAS implémenter (décision finale)

- `detect-eval-with-expr` — couvert par `no-eval`
- `detect-non-literal-require` — couvert par `import-no-commonjs`
- `html-no-target-blank` — obsolète (browsers modernes ajoutent noopener auto)
- `import-no-restricted-paths` — trop config-heavy
- `enforce-logical-properties` — trop niche (RTL)
- `tailwind-no-custom-classname` — nécessite config Tailwind
- `boundaries-*` — trop config-heavy
- `index-only-import-export` — on ne veut pas de barrels
- `pkg-valid-semver` — npm le catch déjà
- `pkg-absolute-version` — trop opinionated
- `vitest-no-large-snapshots` — threshold arbitraire
- `vitest-require-test-timeout` — détection complexe
- `proper-arrows-return` — overlap
- `visual-complexity` — redundant avec cognitive-complexity
- `no-broad-exception` — trop de faux positifs
- `no-null` — déjà existante dans comply
