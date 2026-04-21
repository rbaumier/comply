# Skill-driven rules — implementation report

**Date**: 2026-04-21
**Branch**: `feat/skill-driven-rules`
**Plan**: [`docs/plans/2026-04-21-skill-driven-rules.md`](../plans/2026-04-21-skill-driven-rules.md)
**PRD**: [`docs/prds/2026-04-21-skill-driven-rules.md`](../prds/2026-04-21-skill-driven-rules.md)

## Résumé

80 nouvelles règles natives comply livrées en 15 batches séquentiels, un commit par batch.
Suite de tests : 2940 → 4210 (+1270 tests).

| # | Batch | Règles | Commit |
|---|-------|--------|--------|
| 1 | TypeScript/Architecture | 3 | `1134dfa3` |
| 2 | React | 5 | `1cb91ecd` |
| 3 | Tailwind | 5 | `5e84ba05` |
| 4 | Database SQL | 4 | `c42b2593` |
| 5 | Rust | 7 | `62dc16cc` |
| 6 | TanStack Start | 4 | `3c847982` |
| 7 | TanStack Query | 10 | `71a7dc01` |
| 8 | API Design | 3 | `bfd92b7a` |
| 9 | Zod | 7 | `5f685f1f` |
| 10 | Vue | 7 | `1a85a92a` |
| 11 | i18n | 5 | `e95e9c54` |
| 12 | Security | 7 | `03ab133d` |
| 13 | Better Auth | 5 | `678baa86` |
| 14 | Testing | 4 | `92d415ff` |
| 15 | Drizzle ORM | 4 | `07e1d132` |

---

## Méthode commune

Toutes les règles suivent l'architecture existante de comply :

- **Un répertoire par règle** sous `src/rules/{snake_case}/`
- **`mod.rs`** avec `RuleMeta` (id, description, remediation, severity, categories, doc_url) et `register()`
- **`typescript.rs`** (AstCheck tree-sitter) ou **`rust.rs`** ou **`text.rs`** (TextCheck) pour la détection
- **Tests inline** dans le backend, via `run_ts()`, `run_tsx()`, `run_rust()` de `test_helpers.rs`
- **Enregistrement** dans `src/rules/mod.rs` (`pub mod` + entrée dans `all_rule_defs()` avant `rules.extend(delegated::register_all())`)

Aucun nouveau concept introduit — ni type de backend, ni macro, ni helper générique.
Un seul ajout d'infrastructure : `run_rust_with_path()` dans `test_helpers.rs` (Batch 5), pour tester les règles qui filtrent par chemin (`src/main.rs` vs crates bibliothèques).

**Sévérités** :
- `Error` — bug potentiel runtime ou faille sécurité
- `Warning` — best practice ou convention

**Tests** : chaque règle livre au minimum un test de violation + un test de non-violation. Les règles avec conditions contextuelles (file-type, path) ajoutent un test par variante.

---

## Batch 1 — TypeScript/Architecture (3 règles)

Commit `1134dfa3`. Backend : AstCheck tree-sitter TS/JS/TSX.

| Rule ID | Backend | Détection |
|---------|---------|-----------|
| `no-default-export` | AST | Node `export_statement` avec mot-clé `default`, hors `*.stories.tsx` |
| `prefer-promise-all` | AST | 2+ `await` séquentiels indépendants (pas de dépendance entre les promesses) |
| `ts-prefer-using-declaration` | AST | Pattern `try { … } finally { x.close()/dispose()/… }` → suggérer `using` |

---

## Batch 2 — React (5 règles)

Commit `1cb91ecd`. Backend : mélange AstCheck TSX et TextCheck.

| Rule ID | Backend | Détection |
|---------|---------|-----------|
| `react-no-derived-state-in-effect` | AST TSX | `useEffect` qui fait uniquement `setState` à partir de props — suggérer calcul inline ou `useMemo` |
| `react-no-inline-default-prop` | AST TSX | `prop = prop ?? { … }` / `?? []` dans un composant — réintroduit nouveau ref à chaque render |
| `react-passive-event-listeners` | AST TSX | `addEventListener('touchstart'/'wheel', …)` sans `{ passive: true }` |
| `react-prefer-use-transition` | Text | `setIsLoading(true); await …; setIsLoading(false)` → suggérer `useTransition` |
| `react-server-action-requires-auth` | Text | Fichier `'use server'` avec mutation DB sans appel `auth()`/`getSession()` |

---

## Batch 3 — Tailwind (5 règles)

Commit `5e84ba05`. Backend : TextCheck (scanne `className="…"`, `@apply`).

| Rule ID | Détection |
|---------|-----------|
| `tailwind-no-important-modifier` | Modifier `!` dans une classe (`!text-red-500`) |
| `tailwind-no-arbitrary-z-index` | Valeur arbitraire `z-[n]` — suggérer un token |
| `tailwind-prefer-size-shorthand` | `w-4 h-4` côte à côte → suggérer `size-4` |
| `tailwind-no-apply-for-variants` | `@apply` avec modifier (`hover:`, `md:`, …) dans un bloc `@apply` |
| `tailwind-prefer-cn-utility` | `className={\`… ${cond ? 'x' : 'y'} …\`}` sans `cn()` |

**Ajustements subagent** :
- `tailwind_prefer_size_shorthand` : extraction explicite du contenu de `className="…"` avant `split_whitespace` (le plan laissait `className="w-4` attaché).
- `tailwind_no_apply_for_variants` : tracker de profondeur d'accolades char-par-char (le `starts_with("@apply")` du plan ratait `.btn { @apply px-4; }`).
- `tailwind_prefer_cn_utility` : utilisation des helpers partagés `jsx_attribute_name` / `jsx_attribute_value` de `src/rules/jsx.rs` (les champs `"name"`/`"value"` n'existent pas sur `jsx_attribute` en tree-sitter TSX).

---

## Batch 4 — Database SQL (4 règles)

Commit `c42b2593`. Backend : TextCheck sur fichiers `.sql`.

| Rule ID | Détection |
|---------|-----------|
| `sql-create-index-concurrently` | `CREATE INDEX` sans `CONCURRENTLY` — prévient les verrous de table en prod |
| `sql-nullable-requires-comment` | Colonne `NULL`-able sans commentaire explicatif au-dessus |
| `sql-advisory-lock-prefer-xact` | `pg_advisory_lock()` — préférer `pg_advisory_xact_lock()` |
| `sql-require-transaction-timeout` | `BEGIN;` sans `SET LOCAL statement_timeout = …` |

**Ajustement subagent** : `sql-create-index-concurrently` utilise `upper.contains("CREATE INDEX")` (plutôt que `starts_with`) pour supporter les contextes template-literal JS.

---

## Batch 5 — Rust (7 règles)

Commit `62dc16cc`. Backend : AstCheck tree-sitter Rust pour 1 règle, TextCheck pour les 6 autres.

| Rule ID | Backend | Détection |
|---------|---------|-----------|
| `rust-prefer-once-lock` | Text | `lazy_static!` ou `once_cell::sync::Lazy` → suggérer `std::sync::OnceLock`/`LazyLock` |
| `rust-vec-with-capacity` | Text | `Vec::new()` dans une boucle — suggérer `Vec::with_capacity(n)` |
| `rust-prefer-channel-over-arc-mutex-vec` | Text | Pattern `Arc<Mutex<Vec<T>>>` pour collecter résultats de tâches → suggérer `mpsc::channel` |
| `rust-anyhow-context-on-question-mark` | Text | `?` sur `Result` sans `.context()`/`.with_context()`, uniquement dans crates applicatives (détectées via `src/main.rs`) |
| `rust-must-use-on-result-fn` | AST Rust | `pub fn` retournant `Result<T, E>` sans `#[must_use]` |
| `rust-unsafe-ffi-isolation` | Text | Bloc `unsafe` contenant un appel FFI `extern "C"` sans isolation dans une fonction safe |
| `rust-thiserror-for-lib` | Text | Crate bibliothèque utilisant `anyhow::Error` en type public → préférer `thiserror` |

**Ajustements subagent** :
- `run_rust_with_path` ajouté à `test_helpers.rs` (nécessaire pour les règles qui filtrent par chemin ; non présent avant).
- `rust-must-use-on-result-fn` : helper `is_pub()` itérant les enfants (le champ `visibility_modifier` n'existe pas comme named field en tree-sitter-rust).

---

## Batch 6 — TanStack Start (4 règles)

Commit `3c847982`. Backend : TextCheck TS+TSX+JS.

| Rule ID | Détection |
|---------|-----------|
| `tanstack-start-server-fn-requires-validation` | `createServerFn` sans `.safeParse`/`.parse` sur les arguments |
| `tanstack-start-server-fn-requires-auth` | `createServerFn` avec mutation sans appel d'auth |
| `tanstack-start-server-fn-file-convention` | Fichier exportant `createServerFn` mais pas nommé `*.functions.ts` |
| `tanstack-start-require-validate-search` | `createFileRoute` sans `validateSearch:` |

---

## Batch 7 — TanStack Query (10 règles)

Commit `71a7dc01`. Backend : TextCheck TS+TSX+JS.
Le plus gros batch — 6 règles de renommage v5 + 4 règles de best practice.

**Renommages v5 (migration guide)** :

| Rule ID | Pattern déprécié → correct |
|---------|----------------------------|
| `tanstack-query-no-is-loading` | `isLoading` → `isPending` |
| `tanstack-query-no-cache-time` | `cacheTime` → `gcTime` |
| `tanstack-query-no-use-error-boundary` | `useErrorBoundary: true` → `throwOnError: true` |
| `tanstack-query-no-keep-previous-data-prop` | `keepPreviousData: true` → `placeholderData: keepPreviousData` |
| `tanstack-query-no-query-callbacks` | `onSuccess`/`onError`/`onSettled` sur `useQuery` — supprimées en v5 |
| `tanstack-query-no-enabled-true` | `enabled: true` — valeur par défaut, supprimer |

**Best practice** :

| Rule ID | Détection |
|---------|-----------|
| `tanstack-query-require-stale-time` | `new QueryClient()` sans `defaultOptions.queries.staleTime` |
| `tanstack-query-fn-must-throw-on-error` | `queryFn` qui catch une erreur sans throw — TanStack Query ne peut pas détecter l'échec |
| `tanstack-query-prefer-query-options` | 3+ `useQuery` sur la même clé → suggérer `queryOptions()` factory |
| `tanstack-query-prefer-key-factory` | Literal array queryKey inline → suggérer un `keyFactory` |

---

## Batch 8 — API Design (3 règles)

Commit `bfd92b7a`. Backend : TextCheck TS+TSX+JS.

| Rule ID | Détection |
|---------|-----------|
| `api-no-array-root-response` | Handler retournant un tableau racine (`return [...]`) → suggérer enveloppe `{ items: [...] }` |
| `api-list-requires-pagination` | Handler GET list sans `limit`/`cursor`/`page` param |
| `api-import-from-public-index` | Import depuis `/internal/*` au lieu de l'index public |

---

## Batch 9 — Zod (7 règles)

Commit `5f685f1f`. Backend : TextCheck TS+TSX+JS.

| Rule ID | Détection |
|---------|-----------|
| `zod-prefer-safe-parse` | `.parse()` dans un handler/middleware (suggérer `.safeParse()`) |
| `zod-string-min-1-required` | `z.string()` dans un schéma requis sans `.min(1)` |
| `zod-trim-before-min` | `.min(1).trim()` au lieu de `.trim().min(1)` |
| `zod-prefer-discriminated-union` | `z.union()` sur objets partageant une clé constante → suggérer `discriminatedUnion` |
| `zod-refine-requires-path` | `.refine()` cross-field sans `path:` — erreurs s'attachent au root |
| `zod-require-error-messages` | `z.string()/.min()/…` sans message d'erreur custom |
| `zod-no-optional-nullable-chain` | `.optional().nullable()` → suggérer `.nullish()` |

**Ajustement subagent** : `zod_require_error_messages` — `depth.saturating_sub(1)` pour passer clippy `implicit_saturating_sub`.

---

## Batch 10 — Vue (7 règles)

Commit `1a85a92a`. Backend : TextCheck sur fichiers `.vue` (le moteur retourne `None` pour les backends TreeSitter sur `.vue`).

| Rule ID | Détection |
|---------|-----------|
| `vue-script-setup-required` | `<script>` avec `setup()` fn sans attribut `setup` (Options API sneaking back) |
| `vue-sfc-section-order` | Ordre de sections invalide (`<template>` avant `<script>`) |
| `vue-no-v-html-unsafe` | `v-html` sans sanitization adjacente |
| `vue-prefer-v-else` | `v-if="x"` + `v-if="!x"` sur frères → `v-else` |
| `vue-require-lifecycle-cleanup` | `onMounted` ajoutant listener global sans `onUnmounted` cleanup correspondant |
| `vue-pinia-store-to-refs` | `const { x } = useStore()` sans `storeToRefs()` — perte de réactivité |
| `vue-define-emits-typed` | `defineEmits(['name'])` (array) au lieu de `defineEmits<{ name: [] }>()` |

**Ajustement subagent** : `vue_prefer_v_else` — let-chain au lieu de `if let … { if … { … } }` pour clippy `collapsible_if`.

---

## Batch 11 — i18n (5 règles)

Commit `e95e9c54`. Backend : AstCheck TS+TSX pour 4, TextCheck pour 1.

| Rule ID | Backend | Détection |
|---------|---------|-----------|
| `i18n-no-hardcoded-string-in-jsx` | AST TSX | Literal string enfant direct d'un JSX element (distingue de `className`, `href`, `data-*`, …) |
| `i18n-no-concat-translation-key` | AST | `t('prefix.' + dynamic)` — i18next ne peut pas extraire statiquement |
| `i18n-no-string-concat-with-translation` | Text | `t('a') + ' ' + t('b')` → suggérer `t('full')` ou interpolation |
| `i18n-prefer-intl-api` | AST | `.toLocaleDateString()` sans locale explicite |
| `i18n-no-manual-pluralization` | AST | `count === 1 ? 'item' : 'items'` → suggérer `t(key, { count })` |

---

## Batch 12 — Security (7 règles)

Commit `03ab133d`. Backend : TextCheck TS+TSX+JS. Toutes `Severity::Error`.

| Rule ID | Détection |
|---------|-----------|
| `no-mass-assignment` | `...req.body` dans `.set(`/`.values(`/`db.insert(`/`db.update(` |
| `no-open-redirect` | `res.redirect(req.query.*)`/`redirect(searchParams.get())` sans validation |
| `no-error-details-in-response` | `err.message`/`err.stack` dans `Response.json(`/`res.json(` |
| `no-shell-exec` | `exec(\`… ${…}\`)` ou `shell: true` |
| `no-path-traversal` | `fs.readFile(req.params.*)` sans `basename()`/`resolve()` |
| `no-unvalidated-url-redirect` | `location.href = searchParams.get(…)` sans validation |
| `no-prototype-pollution` | `_.merge(target, req.body)` / `Object.assign(t, JSON.parse(…))` |

---

## Batch 13 — Better Auth (5 règles)

Commit `678baa86`. Backend : TextCheck TS+TSX+JS.

| Rule ID | Détection |
|---------|-----------|
| `better-auth-no-disable-csrf` | `disableCSRFCheck: true` |
| `better-auth-no-disable-origin-check` | `disableOriginCheck: true` |
| `better-auth-require-rate-limit` | `betterAuth({ … })` sans `rateLimit` |
| `better-auth-plugin-import-path` | `import … from 'better-auth/plugins'` (barrel) au lieu de chemin spécifique |
| `better-auth-trusted-providers` | `accountLinking: { enabled: true }` sans `trustedProviders` |

---

## Batch 14 — Testing (4 règles)

Commit `92d415ff`. Backend : TextCheck TS+TSX+JS. Toutes filtrent sur `.test.`/`.spec.` dans le chemin.

| Rule ID | Détection |
|---------|-----------|
| `testing-prefer-msw` | `vi.mock('axios')` / `global.fetch = vi.fn()` → suggérer MSW |
| `testing-no-and-in-test-name` | `test('X and Y', …)` → splitter en deux tests |
| `testing-prefer-test-each` | 3+ tests avec préfixe commun ≥ 8 chars → suggérer `test.each([...])` |
| `testing-no-undefined-mock-var` | `vi.mock()` factory référençant `let` module-level hors `vi.hoisted()` |

**Ajustement subagent** :
- Seuil de préfixe commun abaissé 10 → 8 chars (plan avait test + code contradictoires).
- `testing_no_undefined_mock_var` : `strip_prefix("let ")` au lieu de slice manuel (clippy `manual_strip`).
- `testing_no_and_in_test_name` : let-chain au lieu de `if let … { if … }` (clippy `collapsible_if`).

---

## Batch 15 — Drizzle ORM (4 règles)

Commit `07e1d132`. Backend : TextCheck TS+TSX+JS.

| Rule ID | Détection |
|---------|-----------|
| `drizzle-returning-on-insert-update` | `db.insert/update().values/set()` sans `.returning()` |
| `drizzle-no-sql-raw-with-variable` | `sql.raw(variable)` ou `sql.raw(\`… ${x}\`)` — SQL injection |
| `drizzle-no-select-without-limit` | `db.select().from(table)` sans `.limit()` ni `.where()` |
| `drizzle-zod-prefer-generated-schema` | Fichier Drizzle avec `z.object({})` manuel → suggérer `createInsertSchema()` |

---

## Faux positifs connus post-implémentation

Lors du `comply src/` final (comply sur sa propre source) : 13 663 violations trouvées.
Le gros des diagnostics provient de règles TextCheck appliquées à des fichiers Rust où des mots-clés du pattern (`NULL`, `CREATE INDEX` dans des strings de test) ressemblent à du SQL ou autre langage ciblé. À nettoyer ultérieurement via filtrage par langue ou heuristique de sortie.

## Vérification finale

- `cargo nextest run` : 4210/4210 tests passent (~7 s)
- `cargo clippy --all --all-targets -- -D warnings` : 0 nouveau warning introduit par les règles (55 erreurs `dead_code` pré-existantes sur règles orphelines non liées)
- Build release : OK

## Pas implémenté (hors périmètre du plan)

Voir `docs/prds/2026-04-21-skill-driven-rules.md` section "Out of Scope" :
- Règles data-flow cross-file
- LLM-backed rules pour les nouveaux domaines
- Auto-fixes (`--fix`)
- Docker, CI/CD, Kubernetes, Swift
