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

## Crashes corrigés

| Bug | Cause | Fix |
|-----|-------|-----|
| Stack overflow sur `image-charts` | Bundles minifiés (650KB, 1-2 lignes) dépassent la stack 8MB par défaut de rayon lors du parsing tree-sitter | 1. `ALWAYS_SKIP_DIRS` dans `files.rs` : skip `node_modules`, `target`, `dist`, `.git` même sans `.gitignore` — 2. Stack rayon 16MB dans `main.rs` via `ThreadPoolBuilder::new().stack_size(16MB)` |
