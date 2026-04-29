# FP Hunting Log — 2026-04-29

Corrections de faux positifs identifiés en scannant des projets réels dans ~/www.

## Crashes UTF-8 (panics sur caractères multi-byte)

| Règle | Fichier | Bug | Fix |
|-------|---------|-----|-----|
| `api-response-envelope-consistency` | `typescript.rs:114` | `body[i..]` panic sur `à` (byte 77..79) | Guard `is_char_boundary(i)` avant slice |
| `svelte-no-on-colon-directive` | `text.rs:55` | `source[i..]` itère byte par byte | Guard `is_char_boundary(i)`, skip continuation bytes |

## Backends Rust sur des règles TS-only

| Règle | Problème | Fix |
|-------|----------|-----|
| `ts-no-loop-func` | Closures en boucle sont idiomatiques en Rust | Backend Rust supprimé |
| `ts-no-magic-numbers` → `no-magic-numbers` | Suffixes de type (`0usize`, `1f32`) non gérés | Renommé, ajouté `strip_suffix()` pour stripper les suffixes Rust, catégorie → `code-quality` |

## Faux positifs sur fichiers de test / e2e

| Règle | Problème | Fix |
|-------|----------|-----|
| `no-extraneous-import` | Flagge `@playwright/test` dans `e2e/` et `.setup.` | Ajouté `/e2e/` et `.setup.` à `is_test_file()` |
| `perf-route-level-code-split` | Flagge imports `./pages/` dans fichiers e2e | Ajouté garde `is_test_or_e2e()` |

## Faux positifs sur API similaires

| Règle | Problème | Fix |
|-------|----------|-----|
| `no-mutating-methods` | `this.input.fill()` (Playwright Locator) flaggé comme `Array.fill()` | Étendu exemption aux `member_expression` receivers |

## Gate manquant sur dépendance

| Règle | Problème | Fix |
|-------|----------|-----|
| `xstate-spawn-usage` | Flagge `spawn()` Node.js dans projets sans XState | Gate sur `has_dep_or_engine("xstate")` |
| `react-prefer-react-cache` | Tests cassés après gate package.json-only | Tests réécrits avec `TempDir` + package.json `react` |

## Fichiers `.d.ts` lintés inutilement

| Règle | Problème | Fix |
|-------|----------|-----|
| `consistent-type-imports` (oxlint) | Flagge tous les imports dans `.d.ts` — fichiers de déclaration de type par définition | Skip `.d.ts` / `.d.mts` dans `classify()` et `Language::from_path()` dans `files.rs` |

## `assertions-in-tests` — patterns d'assertion non reconnus

| Projet | Pattern | Fix |
|--------|---------|-----|
| tRPC | `expectTypeOf<T>()` (vitest/expect-type) | Ajouté `expectTypeOf(` à `has_assertion()` |
| tRPC | `@ts-expect-error` comme assertion compile-time | Check `body_text.contains("@ts-expect-error")` avant le tree walk |
| Fastify | `t.plan(N)` (Node.js test runner) | Ajouté `.plan(` à `has_assertion()` |
| tRPC/svelte-kit | `page.waitForSelector()` (Playwright) | Ajouté `.waitFor` à `has_assertion()` |
| svelte-kit | Assertions déléguées dans helpers (`run_get_pathname_test(...)`) | Non corrigé — nécessiterait analyse inter-procédurale |

## `unused-enum-member` — enums exportés flaguées inutilement

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `unused-enum-member` | immich | 1197 | Les enums exportés (`export enum CastState { IDLE, PLAYING, ... }`) sont flaguées parce que leurs membres ne sont pas référencés dans le fichier de déclaration. Mais ils sont utilisés cross-file — c'est leur raison d'être. La règle était file-local par design, ce qui est correct pour les enums privées mais faux pour les exportées. | Skip les `enum_declaration` dont le parent est un `export_statement`. Les enums non-exportés continuent d'être vérifiés. | 10 |

## i18n — faux positifs sur syntaxe i18next

| Règle | Projet | Hits | Problème | Fix |
|-------|--------|------|----------|-----|
| `i18n-json-valid-message-syntax` | cal.com | 16096 | cal.com utilise i18next (`{{count}}`, `$t(...)`) et non ICU MessageFormat (`{count}`). Le parser ICU rejette les doubles accolades comme syntaxe invalide. | Skip les strings contenant `{{` (i18next interpolation) ou `$t(` (i18next cross-reference). |

## Règles Rust mal calibrées sur projets réels (ripgrep, ruff)

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `id-length` | ripgrep | 303 | En Rust, `f`, `s`, `v`, `e`, `n`, `m`, `i`, `x`, etc. sont idiomatiques dans les paramètres de fonctions, closures et match arms. La règle flagguait tout identifiant < 2 chars sans distinction. | 1. Ajouté `RUST_IDIOMATIC` (17 noms courants) comme exceptions hard-codées dans le backend Rust. 2. Skip les paramètres de closures, for-loops et if-let (scopes courts où les noms courts sont acceptés). | 13 |
| `inverted-assertion-arguments` | ripgrep | 225 | En Rust, `assert_eq!` n'a **aucune convention** expected/actual (contrairement à Jest/JUnit). Flaguer `assert_eq!(0, count(...))` est du bruit pur. | Backend Rust supprimé — la règle ne s'applique plus qu'à TS/JS où la convention `expect(actual).toBe(expected)` existe. | 0 |
| `no-duplicate-string` | ripgrep | 217 | Les strings de test fixtures (`"homer\nlisa\nmaggie"` × 7) sont flaguées car le backend Rust ne skip pas les fichiers de test. | Ajouté garde `in_test_dir` dans le backend Rust. Note : les tests inline `#[cfg(test)]` dans le même fichier restent flaguées — limitation connue. | — |

## `id-length` TS/JS — callbacks, boucles for, underscore

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `id-length` | shadcn-ui | 486 | Les paramètres de callbacks comme `.map((_, i) =>`, `.sort((a, b) =>`, `.forEach((v) =>` sont flaguées. En JS/TS, les noms courts dans les callbacks sont idiomatiques, tout comme les closures Rust. De même, `for (let i = 0; ...)` flag `i` qui est universel. Enfin, `_` (underscore discard) n'était pas dans les exceptions par défaut. | 1. Ajouté `is_in_callback_or_loop()` dans le backend TS : skip les paramètres d'arrow functions passées en arguments d'un appel (= callbacks), et les variables déclarées dans un `for_statement`/`for_in_statement`. 2. Ajouté `_` aux exceptions par défaut dans `defaults.toml`. | 105 |
| `id-length` | tauri | 424 | Même problème callbacks + fichier `bundle.global.js` minifié (40KB, 1 seule ligne) générant 344 hits à lui seul. | Même fix callbacks + détection de fichiers minifiés (voir ci-dessous). | 33 |

## Fichiers minifiés lintés inutilement

| Bug | Projet | Hits avant | Cause | Fix |
|-----|--------|-----------|-------|-----|
| Toutes les règles sur fichiers minifiés | tauri (`bundle.global.js`) | 344+ (id-length seul) | Les fichiers minifiés (`.min.js`, bundles sur une seule ligne) ne sont pas des fichiers source éditables — les linter génère du bruit pur. `bundle.global.js` = 40KB sur 1 ligne. | Ajouté `is_minified` dans `FileCtx` : détecte les `.min.{js,css,mjs,cjs}` par nom, et les fichiers >4KB avec ≤3 lignes par heuristique. Skip au même niveau que `is_generated` dans `dispatch_with_lang()`. |

## Règles tsgolint dupliquées non désactivées

| Règle | Projet | Hits | Problème | Fix |
|-------|--------|------|----------|-----|
| `explicit-function-return-type` | shadcn-ui | 9713 | La règle comply `ts-explicit-function-return-type` est bien `disabled = true` dans defaults.toml, mais une version tsgolint déléguée avec l'ID `explicit-function-return-type` (sans préfixe `ts-`) existe aussi et n'est **pas** couverte par le disabled. Les deux versions de la même règle coexistent avec des IDs différents. | Ajouté `[rules.explicit-function-return-type] disabled = true` dans defaults.toml. |
| `explicit-module-boundary-types` | shadcn-ui | 8081 | Même problème : comply `ts-explicit-module-boundary-types` disabled mais tsgolint `explicit-module-boundary-types` actif. | Ajouté `[rules.explicit-module-boundary-types] disabled = true` dans defaults.toml. |

## `no-duplicate-string` TS — fichiers de test non skippés

| Règle | Problème | Fix |
|-------|----------|-----|
| `no-duplicate-string` (TS) | Le backend Rust skippait déjà les `in_test_dir` mais le backend TypeScript ne le faisait pas. Les test fixtures et fichiers `.test.ts` génèrent des duplications de strings légitimes (setup data, assertions). | Ajouté garde `ctx.file.path_segments.in_test_dir` dans le backend TS, identique au backend Rust. |

## `in_test_dir` — patterns manquants

| Problème | Fix |
|----------|-----|
| `scan_path()` ne détectait que `/tests/` (pluriel), `/__tests__/`, `.test.`, `.spec.`. Les projets JS/TS utilisent aussi `/test/` (singulier, ex: shadcn-ui), `/fixtures/` (templates de test), `/__mocks__/` (Jest mocks). | Ajouté `/test/`, `/fixtures/`, `/__mocks__/` à la détection `in_test_dir`. |

## Crashes corrigés

| Bug | Cause | Fix |
|-----|-------|-----|
| Stack overflow sur `image-charts` | Bundles minifiés (650KB, 1-2 lignes) dépassent la stack 8MB par défaut de rayon lors du parsing tree-sitter | 1. `ALWAYS_SKIP_DIRS` dans `files.rs` : skip `node_modules`, `target`, `dist`, `.git` même sans `.gitignore` — 2. Stack rayon 16MB dans `main.rs` via `ThreadPoolBuilder::new().stack_size(16MB)` |
## `i18n-json-no-untranslated` — noms propres et termes techniques flaggés

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `i18n-json-no-untranslated` | hoppscotch | 8964 | Des mots uniques comme "Discord", "GitHub", "macOS", "Linux", "CLI", "Spotlight" sont flaggés comme "non traduits" alors qu'ils sont identiques dans toutes les langues (noms propres, marques, termes techniques). La fonction `is_likely_untranslatable` ne détectait pas les mots sans espace. | Ajouté une heuristique : les strings sans espace (un seul mot) sont considérées comme probablement non-traduisibles (noms propres, acronymes, termes techniques). | 6288 |

## Crashes corrigés

| Bug | Cause | Fix |
|-----|-------|-----|
## `rust-unused-dep` — explosion O(n²) dans les workspaces Cargo

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `rust-unused-dep` | zed | 34400 | `collect_unique_roots()` trouvait le `Cargo.toml` **le plus proche** de chaque fichier .rs — c'est-à-dire le `Cargo.toml` per-crate, pas le workspace root. `cargo shear` était donc invoqué une fois par crate (200× pour zed), et chaque invocation remonte au workspace root et rapporte **tous** les findings du workspace. Résultat : 200 × 172 = 34400 doublons au lieu de 172. | Ajouté `find_cargo_workspace_root()` qui remonte au `Cargo.toml` contenant `[workspace]` au lieu de s'arrêter au premier trouvé. Déduplication au niveau workspace → une seule invocation par workspace. | 144 |
| `rust-unused-dep` | nushell | 1353 | Même problème. | Même fix. | 33 |
| `rust-unused-dep` | actix-web | 190 | Même problème. | Même fix. | 19 |

## Crashes corrigés

| Bug | Cause | Fix |
|-----|-------|-----|
| Crash sur `actix-web` | `cargo shear --format=json` retourne du texte non-JSON (stderr) quand le sous-crate n'a pas de Cargo.lock ou que la commande échoue. Le `serde_json::from_slice()?.` propageait l'erreur via `?` jusqu'au `main()`, ce qui crashait comply avec "crashed unexpectedly". | Remplacé le `?` par `let Ok(report) = ... else { return Ok(vec![]); }` — un cargo-shear qui échoue à parser ne doit pas crasher tout comply, on retourne simplement zéro diagnostics pour ce workspace. |
| Crash sur `n8n` | `zod-no-safeparse-without-check` (`typescript.rs:79`) : `preceding.len().saturating_sub(120)` peut tomber au milieu d'un caractère multi-byte (ici `→`, bytes 8364..8367). Le `&preceding[look_start..]` panic sur "byte index is not a char boundary". | Ajouté `safe_boundary()` qui recule au `is_char_boundary` le plus proche. Même fix appliqué à 11 autres règles TextCheck qui partagent le même pattern : `angular-require-onpush`, `api-no-status-in-body`, `hono-jwt-secret-hardcoded`, `hono-no-get-with-body`, `no-side-effects-in-initialization`, `prisma-no-findmany-without-take`, `tanstack-query-dehydrate-no-pending-in-ssr`, `tanstack-query-no-async-query-fn-without-await`, `tanstack-router-search-no-use-state-for-url-state`, `ts-no-floating-promise-in-array-method`, `zod-no-parse-in-render`. |

## `no-test-return-statement` — return dans fonctions imbriquées des tests

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `no-test-return-statement` | swr | 377 | La règle cherche le `return` le plus proche dans un callback `test()`/`it()`, mais ne reconnaît que `arrow_function`, `function_expression` et `function` comme fonctions englobantes. En tree-sitter, `function Page() {}` est un `function_declaration` et `initFocus() { ... }` (méthode raccourcie dans un objet littéral) est un `method_definition` — deux types de nœuds que le walker ignorait. Le walker traversait ces fonctions et remontait jusqu'à l'arrow function du test, flagguant le `return` du composant React ou de la méthode objet comme un return de test. | Ajouté `function_declaration` et `method_definition` au match des fonctions englobantes dans `is_return_in_test_callback()`. | 0 |

## `no-generic-names` — noms génériques dans les fichiers de test et stories

| Règle | Projet | Hits avant | Problème | Fix | Hits après |
|-------|--------|-----------|----------|-----|------------|
| `no-generic-names` | swr | 827 | Dans les fichiers de test, les variables `data`, `value`, `result`, `item` sont idiomatiques — on teste une fonction et on vérifie son retour. Flaguer `const { data } = useSWR(...)` dans un test comme "nom générique" est du bruit pur. De même pour les fichiers `.stories.` qui sont des exemples. | Ajouté garde `in_test_dir \|\| in_storybook` dans `visit_node()`. Note : le guard doit être dans `visit_node()` et non dans `check()` car le moteur utilise un dispatch multiplexé qui appelle `visit_node()` directement sans passer par `check()`. | 311 |
| `no-generic-names` | grafana | 23430 | Même problème à grande échelle — majorité des hits dans les fichiers `.test.` de Grafana. | Même fix. | — |
| `no-generic-names` | storybook | 3115 | Même problème — noms génériques dans fichiers de test et `.stories.`. | Même fix. | — |

## `no-duplicate-string` — fichiers `.stories.` non skippés

| Règle | Projet | Hits avant | Problème | Fix |
|-------|--------|-----------|----------|-----|
| `no-duplicate-string` | storybook | 6172 | Les fichiers `.stories.{ts,tsx,js}` contiennent des strings répétées dans les différentes variantes d'un composant (props, labels, descriptions). Ces fichiers ne sont pas du code de production — ce sont des exemples interactifs. Le skip `in_test_dir` ne les couvrait pas. | Ajouté `ctx.file.path_segments.in_storybook` au guard existant dans le backend TS. |

## `prefer-less-than` — comparaisons variable-vs-littéral flagguées

| Règle | Projet | Hits avant | Problème | Fix |
|-------|--------|-----------|----------|-----|
| `prefer-less-than` | remix | 118 | `if (x > 0)`, `if (arr.length >= 1)`, `const ok = count > 5` étaient flaggés. Ces comparaisons variable-vs-littéral sont universellement écrites sujet-en-premier (`x > 0`) plutôt qu'inversées (`0 < x`). La règle n'a de sens que pour les comparaisons variable-vs-variable. | Ajouté un guard dans les backends Rust et TS : si le côté droit est un littéral (number, string, boolean, null, undefined), on ne flag pas. |

## `comment-prose-quality` — faux lexical illusions sur ponctuation

| Règle | Problème | Fix |
|-------|----------|-----|
| `comment-prose-quality` | Le détecteur de "lexical illusion" (mot répété à la fin d'une ligne et au début de la suivante) flagguait `// }\n// }` — deux lignes de fermeture d'accolades consécutives. Les tokens purement ponctuation (`}`, `]`, `,`) ne sont pas des illusions lexicales. | Ajouté deux gardes : (1) les tokens sans caractères alphabétiques sont ignorés, (2) les lignes d'un seul mot ne déclenchent pas la détection (un mot seul à la fin d'une ligne n'est pas une "illusion" — c'est juste un mot court). |

## `dead-export` — panic UTF-8 sur header `@generated`

| Règle | Problème | Fix |
|-------|----------|-----|
| `dead-export` | `source[..2048]` peut couper au milieu d'un caractère multi-byte quand le fichier contient des caractères non-ASCII dans les 2048 premiers bytes, causant un panic sur la méthode `.contains()` du slice. | Ajouté boucle `while !source.is_char_boundary(end) { end -= 1; }` pour reculer jusqu'à une frontière de caractère valide avant le slice. |

## `no-empty-test-file` — Node.js assert non reconnu comme marqueur de test

| Règle | Problème | Fix |
|-------|----------|-----|
| `no-empty-test-file` | Les fichiers de test utilisant `assert.equal()`, `assert.ok()` (Node.js built-in test runner) n'étaient pas reconnus comme contenant du contenu de test. Seuls `test(`, `it(`, `describe(`, `expect(` étaient dans `TEST_MARKERS`. | Ajouté `assert(` et `assert.` à `TEST_MARKERS`. |
