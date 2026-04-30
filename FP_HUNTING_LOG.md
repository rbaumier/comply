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
`this.input.fill()` (Playwright Locator) flaggé comme `Array.fill()`. La règle ne peut pas distinguer un Array d'un Locator sans type checker (oxc donne les scopes, pas les types). Fix : quand `.fill()` est appelé sur un accès de propriété chaîné (`this.input.fill()`, `page.locator.fill()`), on ne flag pas — les vrais `Array.fill()` sont typiquement sur des variables locales directes.

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

16096 hits. cal.com utilise i18next (`{{count}}`, `$t(...)`) et non ICU MessageFormat (`{count}`). Le parser ICU rejette les doubles accolades comme syntaxe invalide. Fix : skip les strings contenant `{{` (i18next interpolation) ou `$t(` (i18next cross-reference). Exemple : `"event_type_count": "{{count}} type d'événement"` — `{{count}}` est i18next, pas ICU `{count}`. **TODO** : supporter i18next nativement (voir ISS-058).

### `id-length` Rust — noms idiomatiques non exemptés (ripgrep)

303 hits. ~~Fix initial : RUST_IDIOMATIC (17 noms exemptés) + skip closures/for-loops.~~ **Reverté** : les noms courts doivent être flaggés partout. Seule exception : `a`/`b` en paire dans des fonctions/closures à exactement 2 params (pattern sort/compare).

### `inverted-assertion-arguments` Rust (ripgrep)

225 hits. En Rust, `assert_eq!` n'a **aucune convention** expected/actual (contrairement à Jest/JUnit). Flaguer `assert_eq!(0, count(...))` est du bruit pur. Fix : backend Rust supprimé — la règle ne s'applique plus qu'à TS/JS. Après : 0.

### `no-duplicate-string` Rust — fichiers de test non skippés (ripgrep)

217 hits. Les strings de test fixtures (`"homer\nlisa\nmaggie"` × 7) sont flaguées car le backend Rust ne skip pas les fichiers de test. Fix : ajouté garde `in_test_dir` dans le backend Rust. Note : les tests inline `#[cfg(test)]` restaient flaguées (corrigé en session 4).

### `id-length` TS/JS — callbacks, boucles for, underscore (shadcn-ui, tauri)

shadcn-ui : 486 hits. ~~Fix initial : skip callbacks et for-loops en TS.~~ **Reverté** : les noms courts doivent être flaggés y compris dans les callbacks (`.map((x) =>`, `.sort((a, b) =>`). Seul `_` reste dans les exceptions par défaut.

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

swr : 827 hits → 311. Fix : skip `in_test_dir || in_storybook` dans `visit_node()`. En dehors des tests, `const { data } = useQuery()` est flaggé — le dev doit renommer : `const { data: profiles } = useQuery()`.

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

402 hits → 207. ~~Fix initial : skip les classes décorées.~~ **Reverté** : les décorateurs ne changent rien — une classe vide est une classe vide, décorée ou non.

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

838 hits. ~~Fix initial : RUST_IDIOMATIC étendu à 22 noms.~~ **Reverté** : la liste RUST_IDIOMATIC a été supprimée. Seule exception : `a`/`b` quand ils apparaissent ensemble comme les 2 seuls params d'une fonction ou closure (pattern sort/compare, ex: `fn cmp(a: &i32, b: &i32)`, `|a, b| a.cmp(b)`).

### `comment-prose-quality` — convention rustdoc `# Heading` / `Heading if…` (polars)

395 hits → 309 (-86). Les commentaires rustdoc suivent la convention `/// # Panics\n/// Panics if the buffer is empty.` — le mot du heading est répété au début de la ligne suivante. C'est la structure standard de la documentation Rust (cf. rustdoc book), pas une illusion lexicale. 68 hits `Panics`, 6 `Returns`, 6 `Errors`, 6 `panics`. Le code de détection des illusions lexicales est dupliqué entre `text.rs` (backend Vue) et `lint_comment_nodes` dans `mod.rs` (backend AST pour Rust/TS) — le fix initial dans `text.rs` ne couvrait pas les fichiers Rust. Fix : ajouté détection `is_heading_echo` dans `lint_comment_nodes` (mod.rs) — si la ligne précédente est un heading rustdoc (`# …`) de 2 mots et que le premier mot de la ligne courante est identique, on skip.

### `no-duplicate-string` — strings dans `#[cfg(test)]` et `#[test]` non skippées (starship)

895 hits → ~7. Le backend Rust skippait les fichiers dans `/tests/` (`in_test_dir`), mais pas les strings à l'intérieur des modules `#[cfg(test)]` inline dans `src/`. Starship utilise des modules de test inline dans chaque fichier source. 886 des 895 hits étaient dans des fonctions `#[test]` — les strings (`"AWS_REGION"` × 16, `"AWS_PROFILE"` × 26, `"c++ --version"` × 6) sont des fixtures de test. Fix : ajouté `is_in_test_context(node, source)` dans la boucle de `collect_diagnostics` (mod.rs) pour les fichiers Rust. Réutilise le helper existant `rust_helpers::is_in_test_context` qui détecte `#[cfg(test)]`, `#[test]` et `#![cfg(test)]`. Ceci corrige aussi la limitation documentée depuis la session 1 sur ripgrep.

---

## Session 5 — 2026-04-30 (just, hyperfine, date-fns)

### `no-confidential-logging` — macros custom `error!` dans du code test (just)

24 → 0. Le backend Rust matche les noms de macros shorthand (`error`, `warn`, `info`, etc.) sans vérifier le crate d'origine. Dans `just`, `error!` est une macro de test custom (syntaxe struct-like `error! { name: ..., input: ..., kind: ... }`) — pas du logging. Tous les 24 hits étaient dans des modules `#[cfg(test)]` (lexer.rs, parser.rs). Fix : ajouté `is_in_test_context` en early return dans le backend Rust (`no_confidential_logging/rust.rs`).

### `no-and-in-function-name` — fonctions de test avec `_and_` dans le nom (just, hyperfine)

just : 19 → 1 (-18). hyperfine : 8 → 6 (-2). Les fonctions `#[test]` utilisent des noms descriptifs comme `takes_both_preparation_and_conclusion_command_into_account_for_computing_number_of_runs` — c'est une convention de nommage des tests, pas une violation CQS. Le backend TS skip aussi les fichiers `in_test_dir`. Fix Rust : ajouté `has_test_attribute(node)` + `is_in_test_context(node)` en early return (`no_and_in_function_name/rust.rs`). Fix TS : ajouté `in_test_dir` guard (`no_and_in_function_name/typescript.rs`).

### `no-self-import` — bug de comparaison de chemin sur les fichiers `index.ts` (date-fns)

435 → 1 (-434). Le check comparait uniquement le `file_stem()` (`"index"`) de l'import résolu avec celui du fichier courant. Tous les fichiers `src/locale/*/index.ts` de date-fns qui importent `./_lib/formatDistance/index.ts` étaient flaggés parce que `Path::file_stem("_lib/formatDistance/index.ts")` retourne `"index"` — qui matche le stem du fichier importeur. Fix : ajouté un guard `!import_stem.contains('/')` — si l'import traverse un sous-dossier, il ne peut pas être un self-import (`no_self_import/typescript.rs`).

### `rust-serde-deny-unknown-fields` — test structs flaggées (serde)

181 → 4 (-177). Les structs `#[derive(Deserialize)]` dans `test_suite/tests/` étaient flaggées pour l'absence de `#[serde(deny_unknown_fields)]`. Les structs de test n'ont pas besoin de ce guard — ce sont des fixtures, pas des contrats d'API. Fix : ajouté `in_test_dir` guard dans `visit_node` (`rust_serde_deny_unknown_fields/rust.rs`).

### `no-magic-numbers` / `in_test_dir` — fichiers nommés `test.ts` non détectés (date-fns)

5256 → 376 (-4880, **-93%**). date-fns structure ses tests comme `src/endOfWeek/test.ts` — le fichier s'appelle `test.ts` mais n'est pas dans un dossier `/test/` ni nommé `endOfWeek.test.ts`. Le détecteur `in_test_dir` dans `file_ctx.rs` ne matchait pas ce pattern. Fix : ajouté `lower.ends_with("/test.ts")`, `/test.tsx`, `/test.js`, `/test.jsx` et les variantes sans préfixe de chemin. Cela corrige `no-magic-numbers` et toutes les autres règles qui utilisent `in_test_dir`.

---

## Session 6 — 2026-04-30

**Projets scannés** : solid (packages/solid/src/), fd (src/), create-t3-app, actix-web (actix-web/src/), hyper (src/), serde, tokio

### `typescript/no-non-null-assertion` — doublon avec `ts-no-non-null-assertion` (solid)

238 → 119 (-119, **-50%**). Comply possède sa propre règle `ts-no-non-null-assertion` (dans `src/rules/ts_no_non_null_assertion/`), mais la même vérification était aussi déléguée à oxlint sous le nom `typescript/no-non-null-assertion` (dans `src/rules/delegated/ts.rs`). Résultat : chaque assertion `!` était flaggée **deux fois** — un diagnostic de comply, un d'oxlint. Fix : supprimé l'entrée `typescript/no-non-null-assertion` de `delegated/ts.rs` puisque la règle native comply est suffisante.

### `id-length` — closures et paramètres `fmt` flaggés en Rust (fd, tokio, serde, actix, hyper)

fd : 48 → 16 (-32, **-67%**). Impact estimé sur serde : ~200+ FP éliminés.

Deux patterns idiomatiques Rust étaient flaggés :

1. **Paramètres de closures** (`|e|`, `|x|`, `|c|`, `|m|`) — les closures Rust ont un scope de 1-3 lignes. Les noms single-letter y sont la norme (`vec.iter().map(|x| x + 1)`, `result.map_err(|e| e.to_string())`). Couvre les closures typées (`|a: &i32, b: &i32|`) et non-typées (`|x|`). Fix : ajouté `is_closure_param(node)` qui détecte si le parent (ou grandparent pour les closures typées) est un `closure_parameters`.

2. **Paramètre `f` dans `fn fmt(&self, f: &mut Formatter)`** — c'est LA convention universelle pour implémenter `Display`, `Debug`, `Write` en Rust. Chaque impl de trait formatting utilise `f`. Fix : ajouté `is_fmt_param(node)` qui détecte un paramètre dans une fonction nommée `fmt`.

Note : les ~192 `f` restant sur tokio sont des **function params** de higher-order functions (`fn map(self, f: F)`), pas des closures ni des `fmt`. C'est aussi idiomatique mais plus nuancé — non corrigé pour l'instant.

### `no-abbreviated-names` — `addr` flaggé en Rust (actix-web)

31 → 0 (-31, **-100%**). `addr` est une abréviation standard dans l'écosystème Rust : `std::net::SocketAddr`, `peer_addr()`, `local_addr()`, `bind_addr`. Tous les hits sur actix-web étaient des usages parfaitement idiomatiques. Fix : retiré `addr` de la liste `BANNED_ABBREVIATIONS` dans `no_abbreviated_names/rust.rs`, avec commentaire expliquant pourquoi (même traitement que `ctx`, `idx`, `err`, `fmt` déjà exemptés).

### `no-hardcoded-ip` — IPs de documentation RFC 5737 flaggées (actix-web)

28 → 6 (-22, **-79%**). Les IPs `192.0.2.x` (TEST-NET-1), `198.51.100.x` (TEST-NET-2), et `203.0.113.x` (TEST-NET-3) sont des plages RFC 5737 réservées **exclusivement** à la documentation et aux exemples. Elles ne correspondent jamais à de vraies machines. actix-web les utilise dans ses tests et exemples de parsing HTTP (headers `Forwarded`, `X-Forwarded-For`). Fix : ajouté `is_documentation_ip()` qui détecte les 3 ranges RFC 5737, appelé dans la boucle de détection (`no_hardcoded_ip/text.rs`).

### `no-duplicate-string` — strings dans les attributs Rust `#[cfg(...)]` (diesel)

776 → 108 (-668, **-86%**). Les strings dans les attributs Rust (`#[cfg(feature = "postgres_backend")]`, `#[cfg_attr(...)]`, `#[serde(rename = "...")]`) sont de la métadata de compilation — elles ne **peuvent pas** être extraites dans une `const` (la syntaxe des attributs Rust n'accepte pas de références à des constantes). Diesel utilise massivement `cfg_attr` pour le support multi-backend (PostgreSQL, MySQL, SQLite). Fix : ajouté `"attribute_item" | "inner_attribute_item" => return true` dans `should_ignore_string_node` (`no_duplicate_string/mod.rs`). Le fix s'applique aussi aux projets TS via les nœuds `decorator`.

### `no-bitwise-in-boolean` — bitmask tests flaggés en Rust et TS (crossbeam)

35 → 0 (-35, **-100%**). L'expression `state & FLAG == 0` est le pattern standard pour tester un bit flag atomique. En Rust, `&` a une priorité plus haute que `==`, donc c'est `(state & FLAG) == 0` — un test de bitmask intentionnel, pas une confusion `&&`/`||`. La règle flaggait tout opérateur bitwise dans un if/while, même quand il fait partie d'une comparaison. Fix (Rust + TS) : ajouté `COMPARISON_OPS` — si l'opération bitwise est à l'intérieur d'une `binary_expression` avec `==`/`!=`/`<`/`>`/etc., c'est un bitmask test et on ne flag pas (`no_bitwise_in_boolean/rust.rs`, `no_bitwise_in_boolean/typescript.rs`).

### Bilan session 6

| Règle | Projet | Avant | Après | FP éliminés |
|---|---|---|---|---|
| `typescript/no-non-null-assertion` (doublon) | solid | 238 | 119 | -119 |
| `id-length` (closures + fmt) | fd | 48 | 16 | -32 |
| `no-abbreviated-names` (`addr`) | actix-web | 31 | 0 | -31 |
| `no-hardcoded-ip` (RFC 5737) | actix-web | 28 | 6 | -22 |
| `no-duplicate-string` (attributs Rust) | diesel | 776 | 108 | -668 |
| `no-bitwise-in-boolean` (bitmask tests) | crossbeam | 35 | 0 | -35 |
| **Total estimé** | | | | **~907+** |
