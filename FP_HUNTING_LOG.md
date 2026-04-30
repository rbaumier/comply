# FP Hunting Log

Corrections de faux positifs identifiés en scannant des projets réels dans `test-projects/`.

---

## Session 1 — 2026-04-29

### Crashes UTF-8 (panics sur caractères multi-byte)

**`api-response-envelope-consistency`** (`typescript.rs:114`)
`body[i..]` panic sur `à` (byte 77..79). Fix : guard `is_char_boundary(i)` avant slice.

**`svelte-no-on-colon-directive`** (`text.rs:55`)
`source[i..]` itère byte par byte. Fix : guard `is_char_boundary(i)`, skip continuation bytes.

### Backends Rust sur des règles TS-only

**`ts-no-loop-func`**
Closures en boucle sont idiomatiques en Rust. Fix : backend Rust supprimé.

**`ts-no-magic-numbers` → `no-magic-numbers`**
Suffixes de type (`0usize`, `1f32`) non gérés. Fix : renommé, ajouté `strip_suffix()` pour stripper les suffixes Rust, catégorie → `code-quality`.

### Faux positifs sur fichiers de test / e2e

**`no-extraneous-import`**
Flagge `@playwright/test` dans `e2e/` et `.setup.`. Fix : ajouté `/e2e/` et `.setup.` à `is_test_file()`.

**`perf-route-level-code-split`**
Flagge imports `./pages/` dans fichiers e2e. Fix : ajouté garde `is_test_or_e2e()`.

### Faux positifs sur API similaires

**`no-mutating-methods`**
`this.input.fill()` (Playwright Locator) flaggé comme `Array.fill()`. Fix : étendu exemption aux `member_expression` receivers.

### Gate manquant sur dépendance

**`xstate-spawn-usage`**
Flagge `spawn()` Node.js dans projets sans XState. Fix : gate sur `has_dep_or_engine("xstate")`.

**`react-prefer-react-cache`**
Tests cassés après gate package.json-only. Fix : tests réécrits avec `TempDir` + package.json `react`.

### Fichiers `.d.ts` lintés inutilement

**`consistent-type-imports`** (oxlint) — shadcn-ui
Flagge tous les imports dans `.d.ts` — fichiers de déclaration de type par définition. Fix : skip `.d.ts` / `.d.mts` dans `classify()` et `Language::from_path()`.

### `assertions-in-tests` — patterns d'assertion non reconnus

**tRPC** — `expectTypeOf<T>()` (vitest/expect-type) non reconnu. Fix : ajouté à `has_assertion()`.

**tRPC** — `@ts-expect-error` comme assertion compile-time non reconnu. Fix : check `body_text.contains("@ts-expect-error")` avant le tree walk.

**Fastify** — `t.plan(N)` (Node.js test runner) non reconnu. Fix : ajouté `.plan(` à `has_assertion()`.

**tRPC/svelte-kit** — `page.waitForSelector()` (Playwright) non reconnu. Fix : ajouté `.waitFor` à `has_assertion()`.

**svelte-kit** — assertions déléguées dans helpers (`run_get_pathname_test(...)`). Non corrigé — nécessiterait analyse inter-procédurale.

### `unused-enum-member` — enums exportés (immich)

1197 hits. Les enums exportés (`export enum CastState { IDLE, PLAYING, ... }`) sont flaggés parce que leurs membres ne sont pas référencés dans le fichier de déclaration. Mais ils sont utilisés cross-file. La règle était file-local par design — correct pour les enums privées, faux pour les exportées. Fix : skip les `enum_declaration` dont le parent est un `export_statement`. Après : 10.

### `i18n-json-valid-message-syntax` — syntaxe i18next (cal.com)

16096 hits. cal.com utilise i18next (`{{count}}`, `$t(...)`) et non ICU MessageFormat (`{count}`). Le parser ICU rejette les doubles accolades comme syntaxe invalide. Fix : skip les strings contenant `{{` (i18next interpolation) ou `$t(` (i18next cross-reference).

### `id-length` Rust — noms idiomatiques non exemptés (ripgrep)

303 hits. En Rust, `f`, `s`, `v`, `e`, `n`, `m`, `i`, `x`, etc. sont idiomatiques dans les paramètres de fonctions, closures et match arms. La règle flagguait tout identifiant < 2 chars sans distinction. Fix : 1. Ajouté `RUST_IDIOMATIC` (17 noms courants) comme exceptions hard-codées dans le backend Rust. 2. Skip les paramètres de closures, for-loops et if-let (scopes courts). Après : 13.

### `inverted-assertion-arguments` Rust (ripgrep)

225 hits. En Rust, `assert_eq!` n'a **aucune convention** expected/actual (contrairement à Jest/JUnit). Flaguer `assert_eq!(0, count(...))` est du bruit pur. Fix : backend Rust supprimé — la règle ne s'applique plus qu'à TS/JS. Après : 0.

### `no-duplicate-string` Rust — fichiers de test non skippés (ripgrep)

217 hits. Les strings de test fixtures (`"homer\nlisa\nmaggie"` × 7) sont flaguées car le backend Rust ne skip pas les fichiers de test. Fix : ajouté garde `in_test_dir` dans le backend Rust. Note : les tests inline `#[cfg(test)]` restaient flaguées (corrigé en session 4).

### `id-length` TS/JS — callbacks, boucles for, underscore (shadcn-ui, tauri)

shadcn-ui : 486 hits. Les paramètres de callbacks comme `.map((_, i) =>`, `.sort((a, b) =>`, `.forEach((v) =>` sont flaggés. En JS/TS, les noms courts dans les callbacks sont idiomatiques. De même, `for (let i = 0; ...)` flag `i`. Enfin, `_` (underscore discard) manquait des exceptions. Fix : 1. Ajouté `is_in_callback_or_loop()` dans le backend TS. 2. Ajouté `_` aux exceptions par défaut dans `defaults.toml`. Après : 105.

tauri : 424 hits. Même problème callbacks + fichier `bundle.global.js` minifié (40KB, 1 seule ligne) générant 344 hits. Même fix + détection de fichiers minifiés. Après : 33.

### Fichiers minifiés lintés inutilement (tauri)

344+ hits (id-length seul) sur `bundle.global.js`. Les fichiers minifiés (`.min.js`, bundles sur une seule ligne) ne sont pas des fichiers source éditables. Fix : ajouté `is_minified` dans `FileCtx` — détecte `.min.{js,css,mjs,cjs}` par nom, et les fichiers >4KB avec ≤3 lignes par heuristique. Skip au même niveau que `is_generated`.

### Règles tsgolint dupliquées non désactivées (shadcn-ui)

**`explicit-function-return-type`** — 9713 hits. La règle comply `ts-explicit-function-return-type` est `disabled = true` dans defaults.toml, mais une version tsgolint déléguée avec l'ID `explicit-function-return-type` (sans préfixe `ts-`) coexiste et n'est pas couverte par le disabled. Fix : ajouté `[rules.explicit-function-return-type] disabled = true` dans defaults.toml.

**`explicit-module-boundary-types`** — 8081 hits. Même problème. Fix : ajouté `[rules.explicit-module-boundary-types] disabled = true` dans defaults.toml.

### `no-duplicate-string` TS — fichiers de test non skippés

Le backend Rust skippait déjà `in_test_dir` mais le backend TypeScript ne le faisait pas. Les test fixtures et `.test.ts` génèrent des duplications légitimes. Fix : ajouté garde `ctx.file.path_segments.in_test_dir` dans le backend TS.

### `in_test_dir` — patterns manquants

`scan_path()` ne détectait que `/tests/` (pluriel), `/__tests__/`, `.test.`, `.spec.`. Les projets JS/TS utilisent aussi `/test/` (singulier, ex: shadcn-ui), `/fixtures/`, `/__mocks__/`. Fix : ajouté ces 3 patterns.

### Crashes corrigés

**Stack overflow sur `image-charts`** — bundles minifiés (650KB, 1-2 lignes) dépassent la stack 8MB par défaut de rayon lors du parsing tree-sitter. Fix : 1. `ALWAYS_SKIP_DIRS` dans `files.rs` : skip `node_modules`, `target`, `dist`, `.git` même sans `.gitignore`. 2. Stack rayon 16MB via `ThreadPoolBuilder::new().stack_size(16MB)`.

### `i18n-json-no-untranslated` — noms propres et termes techniques (hoppscotch)

8964 hits. Des mots uniques comme "Discord", "GitHub", "macOS", "Linux", "CLI" sont flaggés comme "non traduits" alors qu'ils sont identiques dans toutes les langues. Fix : les strings sans espace (un seul mot) sont considérées comme probablement non-traduisibles. Après : 6288.

### `rust-unused-dep` — explosion O(n²) dans les workspaces Cargo (zed, nushell, actix-web)

zed : 34400 hits. `collect_unique_roots()` trouvait le `Cargo.toml` **le plus proche** de chaque fichier .rs (per-crate, pas workspace root). `cargo shear` était invoqué une fois par crate (200× pour zed), et chaque invocation remonte au workspace root et rapporte tous les findings → 200 × 172 = 34400 doublons. Fix : ajouté `find_cargo_workspace_root()` qui remonte au `Cargo.toml` contenant `[workspace]`. Déduplication au niveau workspace. Après : 144.

nushell : 1353 → 33. actix-web : 190 → 19. Même fix.

### Crashes corrigés (suite)

**Crash sur `actix-web`** — `cargo shear --format=json` retourne du texte non-JSON quand la commande échoue. Le `serde_json::from_slice()?` propageait l'erreur jusqu'au `main()`. Fix : remplacé le `?` par `let Ok(report) = ... else { return Ok(vec![]); }`.

**Crash sur `n8n`** — `zod-no-safeparse-without-check` : `preceding.len().saturating_sub(120)` peut tomber au milieu d'un caractère multi-byte (`→`, bytes 8364..8367). Fix : ajouté `safe_boundary()` qui recule au `is_char_boundary` le plus proche. Même fix appliqué à 11 autres règles TextCheck.

---

## Session 2 — 2026-04-29 (swr, grafana, storybook, remix)

### `no-test-return-statement` (swr)

377 hits → 0. La règle cherche `return` dans un callback `test()`/`it()`, mais ne reconnaît que `arrow_function`, `function_expression` et `function` comme fonctions englobantes. En tree-sitter, `function Page() {}` est un `function_declaration` et `initFocus() { ... }` (méthode raccourcie) est un `method_definition` — le walker traversait ces fonctions et remontait jusqu'à l'arrow du test. Fix : ajouté `function_declaration` et `method_definition` au match.

### `no-generic-names` — noms génériques dans tests et stories (swr, grafana, storybook)

swr : 827 hits → 311. Dans les tests, `data`, `value`, `result`, `item` sont idiomatiques. Flaguer `const { data } = useSWR(...)` dans un test est du bruit pur. Fix : ajouté garde `in_test_dir || in_storybook` dans `visit_node()`.

grafana : 23430 hits. Même problème à grande échelle. Même fix.
storybook : 3115 hits. Même problème. Même fix.

### `no-duplicate-string` — fichiers `.stories.` non skippés (storybook)

6172 hits. Les fichiers `.stories.{ts,tsx,js}` contiennent des strings répétées dans les variantes d'un composant. Ce ne sont pas du code de production. Fix : ajouté `ctx.file.path_segments.in_storybook` au guard existant.

### `prefer-less-than` — comparaisons variable-vs-littéral (remix)

118 hits. `if (x > 0)`, `if (arr.length >= 1)` sont flaggés. Ces comparaisons variable-vs-littéral sont universellement écrites sujet-en-premier (`x > 0`) plutôt qu'inversées (`0 < x`). La règle n'a de sens que variable-vs-variable. Fix : si le côté droit est un littéral (number, string, boolean, null, undefined), on ne flag pas.

### `comment-prose-quality` — faux lexical illusions sur ponctuation

`// }\n// }` flaggé comme illusion lexicale. Fix : (1) tokens sans caractères alphabétiques ignorés, (2) lignes d'un seul mot ne déclenchent pas la détection.

### `dead-export` — panic UTF-8 sur header `@generated`

`source[..2048]` peut couper au milieu d'un caractère multi-byte. Fix : boucle `while !source.is_char_boundary(end) { end -= 1; }`.

### `no-empty-test-file` — Node.js assert non reconnu

Les fichiers utilisant `assert.equal()`, `assert.ok()` (Node.js built-in test runner) n'étaient pas reconnus comme contenant du contenu de test. Fix : ajouté `assert(` et `assert.` à `TEST_MARKERS`.

---

## Session 3 — 2026-04-30 (nest, tokio, axum, bevy, zustand)

### `playwright-expect-expect` — pas de gate sur Playwright (nest)

783 hits → 0. La règle fire sur **tous** les `.test.`/`.spec.` sans vérifier que le projet utilise Playwright. NestJS utilise Jest + supertest. Fix : ajouté gate `source.contains("@playwright/test")`.

### Toutes les règles `playwright-*` — gate systémique manquant

Sur 37 règles `playwright-*`, seule 1 (`playwright-no-hooks`) avait un gate vérifiant `@playwright/test`. Les 36 autres firaient sur tous les fichiers de test peu importe le framework. Projets impactés : zustand (89 FPs), nest (783+), et potentiellement tous les projets TS/JS sans Playwright. Fix : ajouté `source.windows(16).any(|w| w == b"@playwright/test")` dans les 36 règles manquantes.

### `no-extraneous-class` oxlint — doublon non désactivé (nest)

622 hits → 0. La version oxlint-déléguée `no-extraneous-class` coexiste avec la version comply `ts-no-extraneous-class` (qui skip les décorateurs). Fix : `[rules.no-extraneous-class] disabled = true` dans `defaults.toml`.

### `ts-no-extraneous-class` — décorateurs sur `export class` non détectés (nest)

402 hits → 207. Les classes NestJS décorées et exportées (`@Module({...}) export class AppModule {}`) étaient flagguées. En tree-sitter TypeScript, le nœud `decorator` est enfant de `export_statement`, pas de `class_declaration`. Fix : ajouté check du parent `export_statement` pour les décorateurs. Restants = classes réellement vides sans décorateur.

### `rust-no-mutex-in-single-threaded` — fichiers `tests/` standalone (tokio)

83 hits → 74. `is_in_test_context()` ne détecte que `#[cfg(test)]`/`#[test]` dans le AST. Les fichiers standalone dans `tests/` (ex: `tokio/tests/stream_panic.rs`) ne sont pas dans un `#[cfg(test)]` module. Fix : ajouté garde `ctx.file.path_segments.in_test_dir` avant le check AST.

### `timeout-on-io` — tests et exemples (axum)

268 hits → 0. La majorité sont dans les fichiers de test — `client.get("/").await` sur un `TestClient` matche car `client` est dans `IO_BASES` et `.get` dans `IO_METHODS`. Les tests axum utilisent un client HTTP local intégré, pas du vrai I/O réseau. Fix : ajouté skip `in_test_dir`, `is_in_test_context()` et `/examples/`.

### `rust-no-println-in-async` — tests et exemples (axum, tokio)

axum : 38 hits → 0. tokio : 75 hits → 0. `println!` dans les tests async et les exemples est idiomatique. Fix : ajouté skip `in_test_dir`, `is_in_test_context()` et `/examples/`.

### `no-magic-numbers` — exemples non skippés (bevy)

11795 hits → 4220. 64% des hits dans `/examples/` — couleurs, positions, rotations dans des démos interactives. Fix : ajouté skip `/examples/` dans les backends Rust et TypeScript.

---

## Session 4 — 2026-04-30 (polars, starship)

### `id-length` — noms idiomatiques Rust manquants dans RUST_IDIOMATIC (polars)

838 hits → 93. `a` (623 hits), `l` (47), `d` (39), `h` (26), `o` (10) sont des noms universellement idiomatiques en Rust : `a`/`b` pour les paires dans les comparateurs et reducers (`fn combine(a: &mut Value, b: &Value)`), `l`/`r` pour left/right dans les opérations binaires, `d` pour discriminant/deserializer, `h` pour hash/handle, `o` pour other/output. Seuls 17 noms étaient dans `RUST_IDIOMATIC`, il en manquait 5. Fix : ajouté `a`, `d`, `h`, `l`, `o`. La liste est maintenant 22 entrées, triée alphabétiquement.

### `comment-prose-quality` — convention rustdoc `# Heading` / `Heading if…` (polars)

395 hits → 309 (-86). Les commentaires rustdoc suivent la convention `/// # Panics\n/// Panics if the buffer is empty.` — le mot du heading est répété au début de la ligne suivante. C'est la structure standard de la documentation Rust (cf. rustdoc book), pas une illusion lexicale. 68 hits `Panics`, 6 `Returns`, 6 `Errors`, 6 `panics`. Le code de détection des illusions lexicales est dupliqué entre `text.rs` (backend Vue) et `lint_comment_nodes` dans `mod.rs` (backend AST pour Rust/TS) — le fix initial dans `text.rs` ne couvrait pas les fichiers Rust. Fix : ajouté détection `is_heading_echo` dans `lint_comment_nodes` (mod.rs) — si la ligne précédente est un heading rustdoc (`# …`) de 2 mots et que le premier mot de la ligne courante est identique, on skip.

### `no-duplicate-string` — strings dans `#[cfg(test)]` et `#[test]` non skippées (starship)

895 hits → ~7. Le backend Rust skippait les fichiers dans `/tests/` (`in_test_dir`), mais pas les strings à l'intérieur des modules `#[cfg(test)]` inline dans `src/`. Starship utilise des modules de test inline dans chaque fichier source. 886 des 895 hits étaient dans des fonctions `#[test]` — les strings (`"AWS_REGION"` × 16, `"AWS_PROFILE"` × 26, `"c++ --version"` × 6) sont des fixtures de test. Fix : ajouté `is_in_test_context(node, source)` dans la boucle de `collect_diagnostics` (mod.rs) pour les fichiers Rust. Réutilise le helper existant `rust_helpers::is_in_test_context` qui détecte `#[cfg(test)]`, `#[test]` et `#![cfg(test)]`. Ceci corrige aussi la limitation documentée depuis la session 1 sur ripgrep.
