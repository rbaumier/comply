# Règles à implémenter maintenant

90 règles faisables avec tree-sitter (AST-only, pas de type info).

---

## Sécurité (11 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `no-unsafe-shell-exec` | `exec(variable)` — flag si arg non-literal | Medium | CallExpr + arg check |
| `no-postmessage-star-origin` | `postMessage(data, '*')` | Easy | CallExpr `postMessage` + 2nd arg `'*'` |
| `no-document-domain` | `document.domain =` | Easy | AssignExpr lhs `document.domain` |
| `no-document-write` | `document.write()` | Easy | CallExpr `document.write` |
| `no-inner-html` | `innerHTML =` / `outerHTML =` | Easy | AssignExpr lhs ends with `innerHTML`/`outerHTML` |
| `no-insecure-url` | URLs `http://` dans strings | Easy | StringLiteral startsWith `http://` |
| `no-unsafe-alloc` | `Buffer.allocUnsafe()` / `new Buffer(size)` | Easy | CallExpr / NewExpr |
| `detect-dangerous-redirects` | `res.redirect(req.*)` | Medium | CallExpr `redirect` + arg is MemberExpr `req.*` |
| `detect-option-rejectunauthorized` | `{ rejectUnauthorized: false }` | Easy | Property in ObjectLiteral |
| `no-unsafe-regex` | ReDoS patterns | Hard | Regex literal analysis |
| `react-no-javascript-urls` | `href="javascript:..."` JSX | Easy | JSX attr `href` startsWith `javascript:` |

---

## Async / Promises (4 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `no-floating-promise` | Promise non-gérée (heuristique) | Medium | ExprStatement + CallExpr to known async fn |
| `no-async-without-await` | `async` fn sans `await` | Easy | FunctionDecl `async` + no AwaitExpr child |
| `no-async-array-callback` | `arr.forEach(async ...)` | Medium | CallExpr `.forEach/.map/.filter` + async callback |
| `no-redundant-await` | `return await x` fin de fn | Easy | ReturnStatement + AwaitExpr in async fn (not in try) |

---

## Error Handling (6 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `throw-error-values` | `throw 'string'` / `throw {}` | Easy | ThrowStatement + literal/object arg |
| `exception-use-error-cause` | Re-throw sans `{ cause }` | Medium | CatchClause + ThrowStatement `new Error` sans cause |
| `try-catch-json-parse` | `JSON.parse()` hors try | Medium | CallExpr `JSON.parse` + not in TryStatement |
| `try-catch-new-url` | `new URL()` hors try | Medium | NewExpr `URL` + not in TryStatement |
| `no-catch-log-rethrow` | `catch { log(); throw }` | Medium | CatchClause with only log + throw |
| `no-catch-without-use` | `catch(e)` sans utiliser `e` | Easy | CatchClause param not referenced |

---

## Imports (7 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `import-no-cycle` | Imports cycliques | Hard | Cross-file ImportIndex analysis |
| `import-no-extraneous-dependencies` | Import hors package.json | Medium | ImportDecl + package.json lookup |
| `avoid-importing-barrel-files` | Import depuis index.ts | Medium | ImportDecl path ends with `/index` or is dir |
| `import-dedupe` | `import { a, a }` | Easy | ImportDecl duplicate specifiers |
| `no-full-import` | `import _ from 'lodash'` | Easy | ImportDecl default from `lodash`/`underscore` |
| `no-test-imports-in-prod` | Import de `*.test.ts` en prod | Medium | ImportDecl path contains `.test.`/`__mocks__` |
| `require-path-exists` | Path import n'existe pas | Medium | ImportDecl + fs::exists check |

---

## React (9 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `jsx-no-new-function-as-prop` | `onClick={() => ...}` | Easy | JSX attr value is ArrowFunction/FunctionExpr |
| `jsx-ensure-booleans` | `{x && <Y/>}` sans boolean | Medium | JSX LogicalExpr `&&` lhs not comparison/boolean |
| `react-no-adjust-state-on-prop-change` | useEffect setState sur prop | Hard | useEffect + setState + dep is prop |
| `react-no-pass-data-to-parent` | useEffect appelle callback parent | Hard | useEffect + call to prop function |
| `react-no-reset-all-state-on-prop-change` | useEffect reset state sur id | Hard | useEffect + multiple setState + dep is id-like |
| `react-no-chain-state-updates` | Multiple setState dans effect | Medium | useEffect + multiple CallExpr `set*` |
| `react-hook-form-destructuring-formstate` | `formState.isValid` sans destructure | Easy | MemberExpr `formState.*` |
| `no-submit-handler-without-preventDefault` | onSubmit sans preventDefault | Medium | JSX `onSubmit` handler without `preventDefault` call |
| `react-hook-form-*` | Autres règles RHF | Medium | Various patterns |

---

## Database (3 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `enforce-delete-with-where` | `db.delete()` sans `.where()` | Easy | CallExpr `.delete()` not followed by `.where()` |
| `enforce-update-with-where` | `db.update()` sans `.where()` | Easy | CallExpr `.update()` not followed by `.where()` |
| `pg-require-limit` | SELECT sans LIMIT | Medium | SQL string parse or tagged template |

---

## Testing (7 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `playwright-missing-playwright-await` | await manquant sur Playwright | Medium | CallExpr `page.*`/`expect` as ExprStatement |
| `playwright-expect-expect` | Test sans expect | Easy | CallExpr `test`/`it` body without `expect` |
| `playwright-no-eval` | `page.$eval()` | Easy | CallExpr `$eval`/`$$eval` |
| `playwright-prefer-locator` | `page.$()` → locator | Easy | CallExpr `page.$`/`page.$$` |
| `vitest-hoisted-apis-on-top` | `vi.mock` après imports | Medium | CallExpr `vi.mock` position vs ImportDecl |
| `vitest-no-disabled-tests` | `test.skip` / `xtest` | Easy | CallExpr `skip`/`xtest`/`xdescribe` |
| `vitest-no-duplicate-hooks` | Duplicate beforeEach | Easy | Multiple `beforeEach` in same describe |

---

## TypeScript (5 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `ts-consistent-type-imports` | `import type { X }` | Easy | ImportDecl with type specifiers not using `type` |
| `ts-consistent-type-exports` | `export type { X }` | Easy | ExportDecl with type specifiers |
| `ts-no-non-null-assertion` | `value!` | Easy | NonNullAssertion node |
| `ts-only-throw-error` | throw non-Error | Easy | ThrowStatement + literal/object |
| `ts-prefer-promise-reject-errors` | `Promise.reject('msg')` | Easy | CallExpr `Promise.reject` + string arg |

---

## HTML / A11y (10 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `html-no-abstract-roles` | ARIA abstract roles | Easy | Attr `role` in abstract list |
| `html-no-aria-hidden-body` | `aria-hidden` on body | Easy | Element `body` + attr `aria-hidden` |
| `html-no-nested-interactive` | `<button><a>` | Medium | Interactive element contains interactive |
| `html-no-skip-heading-levels` | h1 → h3 | Medium | Track heading levels in document |
| `html-no-positive-tabindex` | tabindex > 0 | Easy | Attr `tabindex` > 0 |
| `html-no-invalid-attr-value` | Invalid attr values | Medium | Attr value not in allowed list |
| `html-require-button-type` | button sans type | Easy | Element `button` without `type` attr |
| `html-require-img-alt` | img sans alt | Easy | Element `img` without `alt` attr |
| `html-require-input-label` | input sans label | Medium | Input without associated label |
| `html-require-explicit-size` | img sans width/height | Easy | Element `img`/`video` without dimensions |

---

## Performance (1 règle)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `require-size-attributes` | JSX img sans dimensions | Easy | JSX `img`/`video` without width/height |

---

## i18n (3 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `i18n-json-identical-keys` | Clés manquantes entre locales | Hard | Cross-file JSON comparison |
| `i18n-json-identical-placeholders` | Placeholders différents | Hard | Cross-file JSON + regex extraction |
| `i18n-json-valid-message-syntax` | Syntaxe ICU invalide | Hard | ICU message format parser |

---

## Code Quality (7 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `no-one-iteration-loop` | Loop qui itère 1 fois | Medium | Loop body unconditional return/break/throw |
| `no-extra-arguments` | Plus d'args que de params | Medium | CallExpr args.len > fn params.len |
| `no-use-of-empty-return-value` | Usage de retour void | Medium | Track fn without return + usage |
| `prefer-early-return` | Guard clause | Medium | If statement wrapping entire fn body |
| `block-scope-case` | case sans {} avec decl | Easy | CaseClause with VariableDecl without Block |
| `max-call-chain-depth` | Trop d'indirections | Medium | Track call graph depth |
| `prefer-single-boolean-return` | `if(x) return true` | Easy | If + return true + else return false |

---

## Immutabilité / FP (5 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `no-mutation` | Mutation sur const | Medium | AssignExpr to const binding |
| `no-mutating-methods` | `.push()/.sort()` etc | Easy | CallExpr in mutating method list |
| `functional-immutable-data` | Mutation de var externe | Hard | AssignExpr to non-local binding |
| `no-delete` | `delete obj.prop` | Easy | DeleteExpr |
| `no-let` | Interdit let | Easy | VariableDecl `let` |

---

## Tailwind (3 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `tailwind-prefer-shorthand` (extend) | `px-2 py-2` → `p-2` | Medium | Class string parse + shorthand map |
| `tailwind-no-deprecated-classes` | Classes v2/v3 dépréciées | Medium | Class string + deprecation list |
| `tailwind-classnames-order` | Ordre canonique | Medium | Class string + sort rules |

---

## Zod (3 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `zod-no-optional-and-default-together` | `.optional().default()` | Easy | CallExpr chain with both |
| `zod-no-unknown-schema` | `z.unknown()` | Easy | CallExpr `z.unknown` |
| `zod-require-schema-suffix` | Export sans `Schema` suffix | Easy | ExportDecl name pattern |

---

## XState (5 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `xstate-entry-exit-action` | Validate entry/exit | Medium | Object property in machine config |
| `xstate-invoke-usage` | Validate invoke | Medium | Object property `invoke` structure |
| `xstate-no-imperative-action` | Interdit send/raise hors action | Medium | CallExpr `send`/`raise` context |
| `xstate-event-names` | SCREAMING_SNAKE events | Easy | String literal in event position |
| `xstate-state-names` | State naming convention | Easy | Object key in states |

---

## Package.json (1 règle)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `pkg-no-dupe-deps` | Dupe dependencies/devDependencies | Easy | JSON parse + key intersection |

---

## Naming / Style (4 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `no-index-file` | Interdit index.ts | Easy | Filename check |
| `top-level-function` | Prefer function decl | Easy | Top-level VariableDecl with arrow |
| `proper-arrows-name` | Arrow assigned to name | Easy | VariableDecl with ArrowFunction |
| `clsx-*` | Règles clsx/cn | Medium | CallExpr `clsx`/`cn` arg patterns |

---

## TOML (nouveau backend) (3 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `toml-no-mixed-type-in-array` | Arrays types mixtes | Medium | TOML array value types |
| `toml-keys-order` | Ordre des clés | Easy | Key sequence |
| `toml-tables-order` | Ordre des tables | Easy | Table sequence |

---

## Markdown (nouveau backend) (2 règles)

| ID | Description | Effort | Pattern |
|----|-------------|--------|---------|
| `markdown-canonical-code-block-language` | `js` → `javascript` | Easy | Code fence language normalization |
| `markdown-*` | Autres règles | Medium | Various |

---

# Résumé

| Effort | Count |
|--------|-------|
| Easy | ~55 |
| Medium | ~30 |
| Hard | ~5 |
| **Total** | **~90** |

## Nouveaux backends requis

1. **TOML** — parser TOML, 3 règles
2. **Markdown** — parser MD, 2+ règles  
3. **HTML pur** — étendre le backend HTML existant pour fichiers .html (pas JSX)

## Ordre d'implémentation suggéré

1. **Sécurité** (11) — haut impact, effort faible
2. **Async** (4) — bugs très courants
3. **Error Handling** (6) — bugs courants
4. **Imports** (7) — utilise ImportIndex existant
5. **React** (9) — communauté large
6. Reste par ordre de priorité dans le doc
