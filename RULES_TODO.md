# Comply — Règles à faire (compilation)

Consolidation des 4 fichiers racine (hors `DIFF_REVIEW_FULL.md` qui est du raw output) :
- `RULES_TO_ADD.md` — nouvelles règles candidates
- `RULES_TO_FIX.md` — règles existantes à corriger
- `TODO.md` — perf debt / tiers / bugs en vrac
- `TODO_AFTER_REVIEWS.md` — classification d'une review POC (bugs + suggestions)
- `DIFF_REVIEW_POC.md` — cas concrets remontés depuis un POC Rust

Format : chaque entrée = une ligne actionnable. À toi de trier / prioriser.

---

## 1. Nouvelles règles — Faciles (AST simple)

| ID | Catégorie | Détecte | Source |
|----|-----------|---------|--------|
| `no-double-cast` | typescript | `as unknown as T` | RULES_TO_ADD |
| `vue-no-mutate-prop` | vue | Assignation à `props.X` | RULES_TO_ADD |
| `drizzle-no-push-in-production` | drizzle (text) | `drizzle-kit push` dans un script CI | RULES_TO_ADD |
| `i18n-no-unnecessary-trans-component` | i18n | `<Trans>` sans enfants JSX | RULES_TO_ADD |
| `i18n-prefer-logical-css-properties` | i18n (text CSS) | `margin-left`/`padding-right` | RULES_TO_ADD |
| `sql-create-index-concurrently` | database (text) | `CREATE INDEX` sans `CONCURRENTLY` | RULES_TO_ADD |
| `sql-advisory-lock-prefer-xact` | database (text) | `pg_advisory_lock(` sans `xact_` | RULES_TO_ADD |

---

## 2. Nouvelles règles — Sécurité (Moyen, cluster cohérent)

Aucune n'est implémentée. Bon candidat pour une session dédiée.

| ID | Détecte |
|----|---------|
| `no-mass-assignment` | `db.update(X).set(req.body)` / `db.insert(X).values(req.body)` |
| `no-open-redirect` | `res.redirect(req.query.returnTo)` sans validation |
| `no-error-details-in-response` | `res.json({ error: err.message })` ou `err.stack` |
| `no-prototype-pollution` | `_.merge(`/spread récursif sur input utilisateur |
| `no-unvalidated-url-redirect` | `window.location = userInput` sans validation protocole |

---

## 3. Nouvelles règles — TypeScript / Architecture (Moyen)

| ID | Détecte |
|----|---------|
| `ts-prefer-satisfies` | `value as Type` remplaçable par `satisfies` |
| `prefer-promise-all` | Awaits séquentiels indépendants |
| `no-unchecked-json-parse` | `JSON.parse(` sans Zod/type guard ensuite |
| `no-conditional-async-return` | Fonction retournant parfois `T` parfois `Promise<T>` |
| `ts-prefer-using-declaration` | `try/finally { close() }` remplaçable par `using` (TS 5.2+) |
| `vertical-slice-no-role-folders` | `src/services/` + `src/repositories/` + `src/handlers/` en parallèle |
| `single-call-site-inline` | Fonction exportée référencée dans 1 seul fichier (Difficile) |

---

## 4. Nouvelles règles — Rust restantes

| ID | Difficulté | Détecte |
|----|------------|---------|
| `rust-prefer-fast-hasher` | Moyen | `HashMap::<u64,` / `HashMap::<usize,` sans `ahash`/`fxhash` |
| `rust-prefer-cow` | Difficile | `&str` puis parfois `.to_owned()` — data-flow |
| `rust-no-mutex-in-single-threaded` | Difficile | `Mutex::new(` sans `spawn`/async — data-flow |

---

## 5. Nouvelles règles — Frameworks (Moyen / Difficile)

### Zod
| ID | Difficulté | Détecte |
|----|------------|---------|
| `zod-transform-requires-pipe` | Moyen | `.transform(fn)` terminal sans `.pipe(z.*)` |
| `zod-brand-ids` | Difficile | `z.string().uuid()` sans `.brand()` pour les IDs |
| `zod-validate-env-at-startup` | Difficile | Accès `process.env.X` sans validation Zod préalable |

### TanStack Query / Start
| ID | Difficulté | Détecte |
|----|------------|---------|
| `tanstack-query-prefer-suspense-query` | Difficile | `useQuery(` dans contexte SSR/RSC |
| `tanstack-start-loader-stale-time` | Moyen | `ensureQueryData(` avec `staleTime` < 5000 |
| `tanstack-start-no-client-import-in-server-fn` | Facile | Import `useState`/`useEffect` dans fichier `.functions.ts` |

### Tailwind
| ID | Difficulté | Détecte |
|----|------------|---------|
| `tailwind-no-magic-spacing` | Moyen | `p-[`, `m-[`, `gap-[` avec px non-multiple de 4 |
| `tailwind-read-theme-before-classes` | Difficile | `bg-blue-500` alors que token `--color-brand-*` existe |

### React
| ID | Difficulté | Détecte |
|----|------------|---------|
| `react-prefer-react-cache` | Difficile | Fonction async appelée depuis plusieurs RSC sans `cache()` |
| `react-no-sequential-await-in-component` | Difficile | `const a = await f(); const b = await g()` sans dépendance |
| `react-hoist-static-jsx` | Difficile | JSX sans props dynamiques créé à l'intérieur d'un composant |

### Vue
| ID | Difficulté | Détecte |
|----|------------|---------|
| `vue-prefer-computed` | Difficile | Expression complexe répétée dans `<template>` sans `computed()` |
| `vue-markraw-for-third-party` | Difficile | Instance de classe tierce assignée à `ref()`/`reactive()` |
| `vue-url-state-for-filters` | Difficile | `useState`-like pour `page`/`sort`/`filter`/`search` |

### Better Auth
| ID | Difficulté | Détecte |
|----|------------|---------|
| `better-auth-require-secure-cookies` | Moyen | Absence de `useSecureCookies: true` dans advanced |
| `better-auth-middleware-requires-headers` | Moyen | `getSession(` dans `middleware.ts` sans `headers: nextHeaders()` |

### Testing
| ID | Difficulté | Détecte |
|----|------------|---------|
| `testing-no-real-external-service` | Moyen | `fetch('https://stripe.com'` dans les tests |

### Drizzle
| ID | Difficulté | Détecte |
|----|------------|---------|
| `drizzle-chunk-large-batch-insert` | Difficile | `db.insert(t).values(array)` sans chunking (limite PG params) |

### API Design
| ID | Difficulté | Détecte |
|----|------------|---------|
| `api-no-boolean-field-in-response` | Difficile | `isX: boolean` dans un DTO de réponse |
| `api-deprecation-headers` | Difficile | Endpoint `@deprecated` sans header `Deprecation`/`Sunset` |

### Database SQL
| ID | Difficulté | Détecte |
|----|------------|---------|
| `sql-require-transaction-timeout` | Difficile | Absence de `idle_in_transaction_session_timeout` dans config |
| `sql-nullable-requires-comment` | Moyen | Colonne `.nullable()` Drizzle sans commentaire justificatif |

---

## 6. Règles existantes — À fixer (décision en attente)

Extraites de `RULES_TO_FIX.md` (§27-§37). Décision encore à prendre.

| # | Règle | Symptôme | Décision actuelle |
|---|-------|----------|-------------------|
| 27 | `no-timing-attack` | Flag `named_children[0].kind() != "index_signature"` (code tree-sitter interne) | À compléter |
| 28 | `no-non-literal-fs-filename` | « impossible de passer que des string literals ? » pertinent ? | À compléter |
| 29 | `blank-line-between-blocks` | TODO_AFTER_REVIEW | À compléter |
| 30 | `intermediate-variables` | Flag du code peu imbriqué comme « deeply nested » | À compléter |
| 31 | `justify-inaction` | Exige commentaire sur `return;` early (ex: `if diagnostics.is_empty()`) | À compléter |
| 32 | `no-hidden-control-flow` | Flag 2 conditions avec `&&` | À compléter |
| 33 | `consistent-assert` | Flag dans le code de tests (`assert!(a == b)`) | À compléter |
| 34 | `catch-error-name` | TODO_AFTER_REVIEW — `Err(e)` idiomatic en Rust | À compléter |
| 35 | `no-zero-fractions` | `1.0` requis pour typage f64 explicite | À compléter |
| 36 | `prefer-simple-condition-first` | `ch as u32 > 0xFFFF \|\| ch == ZWJ` flaggé | À compléter |
| 37 | `comment-prose-quality` | Flag rustdoc valide (`//!`), « actually » interdit, `/` répété flaggé | À compléter |

---

## 7. Bugs remontés dans TODO.md (à investiguer)

### Panic
- `regex-prefer-quantifier/text.rs:47` : panic `byte index 11 is not a char boundary` sur `* .filter(…).shift() — always flag *`

### Règles avec bugs concrets sur `src/main.rs` de comply
- `cognitive-complexity` : donne 6 sur `main()` alors que la spec SonarSource dit 1 (bug dans `FLOW_KINDS` — compte `match_arm`, macro, call, return). Tests à cross-checker TS/Rust.
- `no-small-switch` : « `match` has only 2 arms » — le message parle de « switch » alors qu'on est en Rust.
- `regex-no-duplicate-chars` : flag des lignes qui ne sont PAS des regex (`discovered: &[SourceFile]`, `#[derive(Debug)]`, `fn lint_rust(rs_files: &[&SourceFile], config: &Config)`).
- `regex-sort-flags` / `regex-no-non-standard-flag` / `regex-no-useless-flag` : flaggent des URLs dans des `println!`/`format!` (ex: `https://github.com/...`, `https://docs.anthropic.com/...`).
- `regex-no-misleading-capturing-group` : flag `anyhow::anyhow!("failed to start tokio runtime: {e}")`.
- `regex-no-useless-quantifier` : flag `std::env::current_dir()?`.
- `no-non-literal-fs-filename` : flag `std::fs::write(&target, ...)` — pertinent ?
- `blank-line-between-blocks` : flags pas très précis — à affiner ou drop.
- `justify-inaction` : « early return sans commentaire » — faut-il vraiment un commentaire ?
- `catch-error-name` : force `Err(error)` au lieu de `Err(e)` — pas idiomatic en Rust.
- `comment-prose-quality` :
  - Flag rustdoc valide (`//!` répété = module docstring).
  - « Weasel word 'actually' » trop strict.
  - « `/` répété » sur des `///` successifs — c'est de la rustdoc normale.
- `no-clones` : flag les `META: RuleMeta` de deux règles différentes (sql-no-timestamp-without-tz vs sql-no-varchar) alors que seul le pattern struct est partagé.
- `todo-needs-issue-link` : « a l'air pété » (note brute).
- `no-inconsistent-returns` : « a l'air pété » (note brute).

### Suggestions
- serde/zod cross-pollination : règles serde applicables à zod et vice-versa (ex: `rust-serde-deny-unknown-fields` ↔ `.strict()` zod).
- Ref externe : https://news.ycombinator.com/item?id=47673171, https://github.com/etechlead/token-map (token-map pour la clone detection ?).
- Ref externe : https://philodev.one/posts/2026-04-code-complexity/
- Ref externe : hono — https://www.evlog.dev/

---

## 8. Bugs remontés dans TODO_AFTER_REVIEWS.md

Classification d'une review POC sur la codebase de `poc/query-table`.

### Bugs AST critiques
- **`regex-*` (toutes)** : parseur lit du code/strings standards comme regex. Restreindre au nœud `RegExpLiteral` ou `new RegExp()`.
- **`sql-no-timestamp-without-tz`** : flag `if (type.includes("timestamp"))` en TSX. Restreindre aux templates SQL (`` sql`...` ``) et fichiers `.sql`.
- **`tailwind-no-conflicting-classes`** : `text-xs` (taille) et `text-muted-foreground` (couleur) marqués conflictuels. Bug : suppose que toutes les classes `text-*` sont du même groupe.
- **`no-unthrown-error`** : flag `throw new Error(...)` comme « never thrown ». Bug AST : ne remonte pas au parent `throw`.
- **`generator-without-yield`** : flag `onClick={() => toggle(name)}` (arrow function). Bug AST : confond `ArrowFunctionExpression` avec `FunctionDeclaration generator: true`.
- **`ts-no-invalid-void-type`** : flag `onChange: (value: FilterValue) => void;` comme prop — TypeScript autorise `void` en retour de callback.

### Faux positifs contexte React / Vite
- `import/no-default-export` + `import/prefer-default-export` : règles qui se contredisent. Vite `vite.config.ts` exige `export default`.
- `no-null` : interdit `null` mais React utilise `null` sémantiquement (ErrorBoundary, initial state, refs).
- `no-class-inheritance` : flag `class ErrorBoundary extends Component` — c'est l'unique façon de faire un ErrorBoundary.
- `a11y-click-events-have-key-events` : flag composants Radix/BaseUI qui gèrent déjà le clavier en interne.
- `react-jsx-no-bind` : obsolète — l'impact perf des arrow inline est négligeable sauf sur composants `memo()`.

### Seuils trop stricts
- `no-magic-numbers` : flag `0` — trop strict, à exclure `0`/`1`/`-1`.
- `jsdoc-*` / `module-header` : imposer JSDoc sur 100% des composants React internes = bruit.
- `id-length` (< 2) : flag `(v) => ...`, `(e) => ...` dans lambdas. Ignorer les fonctions inline.
- `max-file-lines` / `max-function-lines` : seuils à 30 lignes trop courts pour du React/JSX. Proposé : 100-150 fn, 250-300 fichier.
- `colocated-tests` : dogmatique sur les composants UI primitifs (shadcn).

---

## 9. Suggestions concrètes remontées dans DIFF_REVIEW_POC.md

### Cas Rust à détecter

1. **Nesting catastrophique → early-return refactor** : le code Rust avec `match Some { if let { match { if let { ... } else { ... } } } None => ... }` doit être refacto en `let Some(x) = foo else { return ... };` + `let Some(pulsar) = &state.pulsar else { ... };` + match tuple. → proche de `llm-pull-complexity-downward`, mais un **AST check** sur `nesting depth ≥ 4` + présence de `if let Some(..) = ...` comme guard serait détectable.

2. **`impl Display` + `impl FromStr` manuels** sur un enum → **doit utiliser `strum`**. Règle suggérée : `rust-prefer-strum` — enum avec `#[derive(...)]` + `impl Display` + `impl FromStr` manuels mirroring des variants 1-pour-1.

3. **2 fonctions presque identiques dans 2 fichiers différents** : doit être flaggé (ne l'est pas aujourd'hui). → couvert par `no-clones` natif mentionné en Tier 0 ci-dessous.

---

## 10. Tier 0 — Performance debt (TODO.md)

### `no-clones` natif (remplace jscpd)

**Status actuel :** jscpd DÉSACTIVÉ dans `src/main.rs` (responsable de 92% du wall-clock sur 216 fichiers). `src/jscpd.rs` compilé sous `#![allow(dead_code)]` comme référence.

**À construire :** rule Rust native, in-process, clone-detection sémantique.

Specs minimales :
- Tokenization via tree-sitter (TS/TSX/JS/Rust), strip identifiers/literals.
- Rolling N-gram fingerprint (N=50 tokens par défaut, = `min-tokens` de jscpd).
- `HashMap<u64, Vec<Location>>` — ≥2 locations = diagnostic.
- **Cross-file, per-run scope** → nouveau `Backend::WholeBatch` ou post-pass phase.
- Language buckets (pas de match TS vs Rust).
- Config `[rules.no-clones] min_tokens` + `ignore` globs dans `comply.toml`.
- Suppression via `// comply-ignore-next-line no-clones`.

**Target perf :** ≤100ms sur 216 fichiers (100× plus rapide que jscpd). Delete `src/jscpd.rs` une fois shippé.

**Acceptance :**
- Rule `no-clones` fire sur fixture integration avec 2 fonctions near-identiques, pass sur negative fixture.
- `tools/bench.ts` : <10% du `engine (rs)` phase sur `many-rules`.
- Re-enable partout où jscpd tournait (voir deleted code dans `lint_typescript`/`lint_rust`).

---

## 11. Tier 3 — Règles nécessitant info de types (pipeline `tsc`)

Requiert une subcommand `comply typecheck` avec `tsc --noEmit`.

| Rule | Approach |
|------|----------|
| `strict-typing` — no inferred `any` | Filter codes TS 7005, 7006, 7031, 7034 |
| `option-vs-result` — `findUser` → `Option<User>` | Signature heuristic sur `find*`/`get*` verbs |
| `misleading-name` — `userList: Set<User>` | Name suffix vs declared type |
| `data-clumps` — same 3+ fields in 2+ types | Cross-file structural match |
| `boundary-condition` — unchecked `arr[0]` / `arr.length - 1` | `noUncheckedIndexedAccess` off → emit |
| `no-raw-db-entity-in-handler` — handler returning Prisma entity | Match against `@prisma/client` types |
| `structured-api-error` — errors need `{type,code,status,detail}` | Shape match |
| `api-first` — handler sans zod/openapi schema adjacent | Text / FS cross-ref |

---

## 12. Tier 5 — LLM / review-only restantes

9 LLM rules déjà en prod. Non-couvertes :

| Rule | Source |
|------|--------|
| `parse-dont-validate` | Philosophy |
| `make-invalid-states-unrepresentable` | Philosophy |
| `functional-core-imperative-shell` | Philosophy |
| `document-impossible-states` | Error Handling |
| `bound-every-input` (rejection at boundary) | Data |
| `crosscutting-via-wrapping` (ex: `withTracing`) | Architecture |
| `map-db-entities-to-dtos` | Architecture |
| `error-messages-as-step-by-step-remediation` | Project Hygiene |

Plus (mémoire `project_comply_next_steps.md`) :
- `command_injection_review` — LLM-level review de chaque `Command::new`/`exec`/`spawn` pour taint depuis user input. AST rule était trop bruyante sans dataflow.
- `path_traversal_review` — LLM-level review de chaque `fs::read`/`fs::write`/`File::open` pour taint depuis user input.

---

## 13. Tier 6 — Architectural / cross-project (LLM)

`llm-temporal-decomposition` et `llm-shallow-module` couvrent déjà temporal decomposition + module depth.

| Rule | Source |
|------|--------|
| `reuse-before-creating` | Philosophy |
| `rule-of-three` | Philosophy |
| `prefer-boring-technology` | Philosophy |
| `dry-repo-wide` | Philosophy |
| `vertical-slices` | Architecture |
| `shotgun-surgery` | Architecture |
| `divergent-change` | Architecture |
| `information-leakage` | Architecture |
| `srp-per-function-module` | Functions |
| `cqs-command-or-query` | Functions |
| `composition-over-inheritance` | Functions |
| `tests-linting-ci-cd-from-day-1` | Project Hygiene |
| `constrain-first-relax-later` | Project Hygiene |
| `codebase-homogeneity` | Project Hygiene |
| `structural-guardrails-over-discipline` | Project Hygiene |
| `hard-cutover-on-migrations` | Project Hygiene |
| `pin-all-versions` | Project Hygiene |
| `group-tests-by-feature-not-type` | File Structure |

---

## 14. eslint-plugin-unicorn — non-implémentables aujourd'hui

Nécessitent des capacités qu'on n'a pas encore.

| Rule | Pre-requisite |
|------|---------------|
| `better-regex` | `regex-syntax` crate + optimizer |
| `consistent-function-scoping` | Scope analysis infra (variable capture detection) |
| `isolated-functions` | Idem |
| `import-style` | Config per-module dans `comply.toml` |
| `no-unnecessary-polyfills` | Browserslist + polyfill DB |
| `no-unused-properties` | Whole-program data-flow analysis |
| `string-content` | Config user-defined dans `comply.toml` |

---

## 15. Méta / tooling

- **Catalog auto-généré** (`project_comply_next_steps.md` #4) : commande `comply catalog` qui génère un markdown/HTML avec id, description, remediation, severity, backend (AST/Text/Clippy/Oxlint/LLM) de chaque règle. Généré depuis les structs `RuleMeta`, pas hand-maintained.
- **Wire des 15 orphan `src/rules/*/rust.rs`** (`project_comply_next_steps.md` #1) : 14 supprimés (doc-only stubs), mais les restants avec vrai Check ont besoin de `mod rust;` + registration. ~10 rules Rust de plus gratos.
