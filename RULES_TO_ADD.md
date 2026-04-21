# Règles comply à implémenter

Générées par comparaison exhaustive skills ↔ règles existantes.
Chaque règle inclut : id kebab-case, catégorie, description, ce qu'elle détecte, faisabilité AST.

Légende faisabilité : **Facile** = pattern AST simple | **Moyen** = pattern composé | **Difficile** = data-flow / heuristique

---

## Zod

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `zod-prefer-safe-parse` | Utiliser `.safeParse()` aux boundaries, pas `.parse()` | `.parse(` appelé dans un handler/route/middleware | Facile |
| `zod-string-min-1-required` | `z.string()` sans `.min(1)` pour les champs obligatoires | `z.string()` nu sans `.min(1)`, `.email()`, `.url()` etc. | Moyen |
| `zod-trim-before-min` | `.trim()` avant `.min(1)` sur les inputs texte utilisateur | `z.string().min(1)` sans `.trim()` précédant | Facile |
| `zod-prefer-discriminated-union` | `z.discriminatedUnion()` quand un champ discriminant existe | `.refine()` sur un `.union()` avec champ `type`/`kind`/`status` commun | Moyen |
| `zod-refine-requires-path` | `.refine()`/`.superRefine()` inter-champs sans `path` | `.refine(fn, { message })` sans `path:` sur un `z.object` | Moyen |
| `zod-brand-ids` | `.brand()` pour les IDs domaine | `z.string().uuid()` sans `.brand()` dans un contexte d'ID utilisateur | Difficile |
| `zod-require-error-messages` | `.refine()` sans message d'erreur | `.refine(fn)` sans deuxième argument | Facile |
| `zod-no-optional-nullable-chain` | `.optional().nullable()` au lieu de `.nullish()` | `.optional().nullable()` ou `.nullable().optional()` chainés | Facile |
| `zod-validate-env-at-startup` | Valider les variables d'environnement au démarrage | Accès `process.env.X` dans le code métier sans validation Zod préalable | Difficile |
| `zod-prefer-z-unknown-over-z-any` | Alias de zod-no-any : `z.unknown()` pas `z.any()` | **Déjà couvert** par `zod-no-any` — vérifier si text-only | — |
| `zod-transform-requires-pipe` | `.transform()` suivi de `.pipe()` pour valider la valeur transformée | `.transform(fn)` terminal sans `.pipe(z.*)` suivant | Moyen |
| `zod-drizzle-prefer-generated-schema` | `createInsertSchema`/`createSelectSchema` au lieu de schémas Zod manuels | Schéma Zod re-déclarant des colonnes Drizzle manuellement | Difficile |

---

## TanStack Query

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `tanstack-query-no-is-loading` | `isLoading` → `isPending` (renommé v5) | `.isLoading` sur résultat `useQuery` | Facile |
| `tanstack-query-no-cache-time` | `cacheTime` → `gcTime` (renommé v5) | `cacheTime:` dans options `useQuery`/`QueryClient` | Facile |
| `tanstack-query-no-use-error-boundary` | `useErrorBoundary` → `throwOnError` (renommé v5) | `useErrorBoundary:` dans options query | Facile |
| `tanstack-query-no-keep-previous-data-prop` | `keepPreviousData: true` → `placeholderData: keepPreviousData` | `keepPreviousData: true` dans options | Facile |
| `tanstack-query-no-query-callbacks` | `onSuccess`/`onError`/`onSettled` supprimés de `useQuery` v5 | `onSuccess:` ou `onError:` sur `useQuery(` | Facile |
| `tanstack-query-require-stale-time` | Définir `staleTime` au niveau `QueryClient` (défaut 0 = toujours refetch) | `new QueryClient(` sans `defaultOptions.queries.staleTime` | Moyen |
| `tanstack-query-fn-must-throw-on-error` | Le `queryFn` doit lancer une erreur si `!res.ok` | `queryFn` avec `fetch(` sans vérification `res.ok` | Moyen |
| `tanstack-query-key-includes-params` | Les paramètres dynamiques doivent figurer dans la query key | `useQuery` avec dépendance sur variable externe absente de `queryKey` | Difficile |
| `tanstack-query-prefer-query-options` | `queryOptions()`/`infiniteQueryOptions()` pour la réutilisation | Options query inline dupliquées dans plusieurs `useQuery` | Difficile |
| `tanstack-query-no-enabled-true` | `enabled: true` est le défaut, redondant | `enabled: true` dans options query | Facile |
| `tanstack-query-prefer-suspense-query` | `useSuspenseQuery` préféré à `useQuery` pour SSR | `useQuery(` dans un contexte SSR/RSC | Difficile |
| `tanstack-query-prefer-key-factory` | Query key factory au lieu de keys inline dispersées | Array literal inline `['resource', id]` dupliqué dans plusieurs fichiers | Difficile |

---

## TanStack Start

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `tanstack-start-server-fn-requires-validation` | `createServerFn` doit valider ses inputs avec Zod | `createServerFn` sans appel `.safeParse`/`.parse` | Moyen |
| `tanstack-start-server-fn-requires-auth` | `createServerFn` doit vérifier l'authentification | `createServerFn` avec mutation sans appel `auth()`/`getSession()` | Moyen |
| `tanstack-start-server-fn-file-convention` | Les server functions dans `.functions.ts` | `createServerFn` dans un fichier ne finissant pas par `.functions.ts` | Facile |
| `tanstack-start-require-validate-search` | `validateSearch` requis sur les routes avec query params | `createFileRoute` accédant à `Route.useSearch()` sans `validateSearch` | Moyen |
| `tanstack-start-loader-stale-time` | `staleTime` du loader ≥ temps de navigation (~5-30s) | `ensureQueryData(` avec `staleTime` inférieur à 5000 | Moyen |
| `tanstack-start-no-client-import-in-server-fn` | Pas d'imports client-only dans `createServerFn` | Import de hooks React/`useState`/`useEffect` dans un fichier `.functions.ts` | Facile |

---

## Vue

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `vue-script-setup-required` | Toujours `<script setup lang="ts">`, jamais Options API `setup()` | `setup()` dans `<script>` sans `setup` attribute | Facile |
| `vue-sfc-section-order` | Ordre SFC : `<script>` → `<template>` → `<style>` | `<template>` avant `<script>` dans un SFC | Facile |
| `vue-no-v-html-unsafe` | `v-html` sans commentaire de sanitization | `v-html` sans `DOMPurify` ou commentaire `// sanitized` adjacent | Moyen |
| `vue-prefer-v-else` | `v-if`/`v-else` en paire, pas deux `v-if` opposés | Deux `v-if` consécutifs avec conditions inverses (`!condition` / `condition`) | Moyen |
| `vue-require-lifecycle-cleanup` | `addEventListener`/`setInterval` dans `onMounted` sans cleanup dans `onUnmounted` | `window.addEventListener(` dans `onMounted` sans `removeEventListener` dans `onUnmounted` | Moyen |
| `vue-pinia-store-to-refs` | `storeToRefs()` pour destructurer un store Pinia | `const { x } = useXStore()` sans `storeToRefs` | Moyen |
| `vue-define-emits-typed` | `defineEmits` avec types génériques | `defineEmits([...])` sans annotation TypeScript générique | Facile |
| `vue-prefer-computed` | Dériver avec `computed()`, ne pas recalculer dans le template | Expression complexe répétée dans `<template>` sans `computed()` correspondant | Difficile |
| `vue-markraw-for-third-party` | `markRaw()` pour les instances de classes tierces (Chart.js, Axios) | Instance de classe tiers assignée à `ref()` ou `reactive()` | Difficile |
| `vue-no-mutate-prop` | Ne pas muter une prop directement | Assignation directe à `props.X = ...` | Facile |
| `vue-url-state-for-filters` | Filtres/tri/pagination dans l'URL, pas dans le state composant | `useState`-like pour `page`, `sort`, `filter`, `search` au niveau composant | Difficile |

---

## Tailwind

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `tailwind-prefer-cn-utility` | `cn()`/`clsx()` pour les classes conditionnelles | Ternaire ou `&&` pour concaténer des classes Tailwind en JSX/TSX | Moyen |
| `tailwind-no-apply-for-variants` | `@apply` seulement pour la couche base, jamais pour les variantes composant | `@apply` dans un fichier composant (pas dans `base` layer) | Moyen |
| `tailwind-no-important-modifier` | Pas de `!` modifier (signe de spécificité mal gérée) | Classe Tailwind commençant par `!` | Facile |
| `tailwind-no-arbitrary-z-index` | Pas de `z-[n]` arbitraire, utiliser des tokens `--z-*` | `z-[` dans les classes | Facile |
| `tailwind-prefer-size-shorthand` | `size-*` quand `w-` et `h-` sont identiques | `w-X` et `h-X` avec la même valeur sur le même élément | Moyen |
| `tailwind-no-magic-spacing` | Valeurs arbitraires `p-[13px]` au lieu des tokens d'espacement | `p-[`, `m-[`, `gap-[` avec valeur px non-multiple de 4 | Moyen |
| `tailwind-read-theme-before-classes` | Utiliser les tokens `@theme` définis, pas les valeurs par défaut | `bg-blue-500` quand un token `--color-brand-*` est défini dans le projet | Difficile |

---

## React (au-delà des règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `react-server-action-requires-validation` | Les Server Actions doivent valider leurs inputs avec Zod | `'use server'` sans `.safeParse`/`.parse` sur les args | Moyen |
| `react-server-action-requires-auth` | Les Server Actions mutantes doivent vérifier l'auth | `'use server'` avec mutation DB sans `getSession()`/`auth()` | Moyen |
| `react-prefer-use-transition` | `useTransition` au lieu de `const [loading, setLoading] = useState(false)` | Pattern `useState(false)` + `setX(true)` avant await + `setX(false)` | Moyen |
| `react-no-inline-default-prop` | Valeur par défaut non-primitive inline (casse la mémoïsation) | Default param `= []`, `= {}`, `= () => {}` dans les props d'un composant `memo()` | Moyen |
| `react-prefer-react-cache` | `React.cache()` pour la déduplication per-request en RSC | Fonction async appelée depuis plusieurs Server Components sans `cache()` | Difficile |
| `react-no-derived-state-in-effect` | Dériver l'état pendant le render, pas dans un `useEffect` | `useEffect` qui appelle uniquement `setState` basé sur des props/state | Moyen |
| `react-passive-event-listeners` | `{ passive: true }` sur les listeners touch/wheel | `addEventListener('touchstart'` ou `addEventListener('wheel'` sans `passive: true` | Facile |
| `react-no-sequential-await-in-component` | Awaits séquentiels pour des fetches indépendants | `const a = await f(); const b = await g()` sans dépendance entre a et b | Difficile |
| `react-use-state-initializer-function` | Lazy initializer pour les valeurs coûteuses ou SSR-unsafe | `useState(localStorage.getItem(...)` ou `useState(buildIndex(items))` | Moyen |
| `react-hoist-static-jsx` | JSX statique hoissé hors du composant | JSX sans props dynamiques créé à l'intérieur d'un composant | Difficile |

---

## i18n

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `i18n-no-hardcoded-string-in-jsx` | Zéro texte utilisateur hardcodé dans JSX | String literal dans JSX enfant direct (pas dans `className`, `href`, etc.) | Moyen |
| `i18n-no-concat-translation-key` | Pas de clé i18n construite par concaténation | `t('prefix.' + variable)` ou template literal dans `t(\`...\`)` | Facile |
| `i18n-no-string-concat-with-translation` | Interpolation i18next au lieu de concaténation | `t('key') + name` ou `t('key') + ' ' + t('other')` | Facile |
| `i18n-prefer-intl-api` | `Intl.DateTimeFormat` au lieu de `.toLocaleDateString()` nu | `.toLocaleDateString()` sans locale explicite | Facile |
| `i18n-no-manual-pluralization` | Pluralisation via `t(key, { count })`, pas `if count === 1` | `count === 1 ? t('singular') : t('plural')` | Moyen |
| `i18n-no-unnecessary-trans-component` | `<Trans>` seulement si interpolation JSX nécessaire | `<Trans>` avec contenu purement textuel (pas de composants enfants) | Facile |
| `i18n-prefer-logical-css-properties` | `margin-inline-start` au lieu de `margin-left` pour RTL | `margin-left`, `padding-right`, `text-align: left`, `border-left` dans CSS | Facile (text) |

---

## Sécurité (au-delà des 35 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `no-prototype-pollution` | Strip `__proto__`/`constructor`/`prototype` avant merge | `_.merge(`, `Object.assign` ou spread récursif sur input utilisateur sans sanitization | Moyen |
| `no-mass-assignment` | Ne jamais spread `req.body` directement dans une DB op | `db.update(X).set(req.body)` ou `db.insert(X).values(req.body)` | Moyen |
| `no-open-redirect` | Valider les URLs de redirection | `res.redirect(req.query.returnTo)` ou `req.query.redirect` sans validation | Moyen |
| `no-error-details-in-response` | Pas de `err.message`/`err.stack` dans le corps de réponse | `res.json({ error: err.message })` ou `{ stack: err.stack }` dans un handler | Moyen |
| `no-shell-exec` | `execFile()` + args array, jamais `exec()` avec input utilisateur | `exec(` avec interpolation de variable, ou `exec(\`...\${var}...\`)` | Moyen |
| `no-path-traversal` | `path.basename()` + vérification prefix pour les chemins user | `fs.readFile(\`/dir/${userInput}\`)` sans `path.basename` ou vérification prefix | Moyen |
| `no-unvalidated-url-redirect` | `new URL(input).protocol` doit être http/https avant utilisation | `window.location = userInput` ou `href={userInput}` sans validation protocole | Moyen |
| `no-ssrf-fetch` | Allowlist les URLs avant fetch côté serveur | `fetch(userProvidedUrl)` dans un handler sans validation de l'URL | Difficile |
| `no-regex-user-input` | Pas d'input utilisateur dans `new RegExp()` | `new RegExp(userInput)` ou `new RegExp(variable)` en dehors de contextes sûrs | Facile (**déjà**: `no-new-regex-with-variable` — vérifier si couvre ce cas) |
| `audit-log-required-fields` | Logs d'audit avec timestamp/userId/action/resource/result | Appel à `logger.info`/`logger.audit` sans les champs obligatoires | Difficile |

---

## TypeScript / Architecture (au-delà des 96 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `no-default-export` | Named exports uniquement, pas de `export default` | `export default function` ou `export default class` | Facile |
| `no-unchecked-json-parse` | `JSON.parse()` sans validation de type | `JSON.parse(` sans appel Zod/type guard sur le résultat | Moyen |
| `ts-prefer-satisfies` | `satisfies` au lieu de `as` pour les assertions de type | `value as Type` quand `satisfies` est applicable | Moyen |
| `no-conditional-async-return` | Pas de return mixte sync/async | Fonction retournant parfois `T`, parfois `Promise<T>` selon une condition | Moyen |
| `prefer-promise-all` | `Promise.all` pour les awaits indépendants séquentiels | `const a = await f(); const b = await g()` sans dépendance a→b | Moyen |
| `ts-prefer-using-declaration` | `using`/`await using` pour le cleanup de ressources (TS 5.2+) | `try { ... } finally { resource.close() }` ou `resource.dispose()` | Moyen |
| `no-double-cast` | Jamais `as unknown as T` | Pattern `as unknown as ` | Facile |

---

## Drizzle ORM (au-delà des 2 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `drizzle-returning-on-insert-update` | `.returning()` après chaque INSERT/UPDATE | `db.insert(` ou `db.update(` sans `.returning()` chaîné | Moyen |
| `drizzle-no-sql-raw-with-variable` | `sql.raw()` interdit avec input utilisateur | `sql.raw(variable)` ou `sql.raw(userInput)` | Facile |
| `drizzle-no-select-without-limit` | Pas de `db.select()` sans `.where()` ni `.limit()` sur les grandes tables | `db.select().from(largeTable)` sans `.where()` ni `.limit()` | Difficile |
| `drizzle-chunk-large-batch-insert` | Chunker les inserts en lot > 500 lignes (limite params PG) | `db.insert(t).values(array)` sans chunking évident | Difficile |
| `drizzle-no-push-in-production` | `drizzle-kit push` interdit en production | `drizzle-kit push` dans un script de déploiement CI | Facile (text) |

---

## Database SQL (au-delà des 15 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `sql-create-index-concurrently` | `CREATE INDEX` doit utiliser `CONCURRENTLY` | `CREATE INDEX` sans `CONCURRENTLY` dans les migrations | Facile (text) |
| `sql-require-transaction-timeout` | `idle_in_transaction_session_timeout` requis dans la config DB | Absence de ce paramètre dans la config de connexion | Difficile |
| `sql-nullable-requires-comment` | Colonne nullable doit avoir un commentaire justificatif | Colonne `.nullable()` Drizzle sans commentaire | Moyen |
| `sql-no-between-timestamp` | `BETWEEN` interdit sur les timestamps (inclusif des deux côtés) | `BETWEEN` avec `TIMESTAMP`/`DATE` dans le SQL | Facile (text) — **déjà**: `sql-no-between-timestamp` — vérifier |
| `sql-advisory-lock-prefer-xact` | `pg_advisory_xact_lock()` plutôt que session-scoped | `pg_advisory_lock(` sans `xact_` | Facile (text) |

---

## API Design (au-delà des 5 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `api-no-array-root-response` | Réponse JSON racine array au lieu d'objet (non extensible) | `return Response.json([` ou `res.json([` dans un handler | Facile |
| `api-list-requires-pagination` | Endpoints list sans pagination | Handler GET retournant un array sans paramètre `cursor`/`page`/`limit` | Moyen |
| `api-no-boolean-field-in-response` | Pas de champ booléen dans une réponse API (préférer enum/status) | `isX: boolean` dans un DTO de réponse | Difficile |
| `api-deprecation-headers` | Headers `Deprecation`/`Sunset` sur les endpoints dépréciés | Endpoint marqué `@deprecated` sans header correspondant dans le handler | Difficile |
| `api-import-from-public-index` | Imports cross-domain uniquement depuis l'index public | Import depuis `features/X/internal/` ou `features/X/db/` depuis un autre domaine | Moyen — **proche de** `layer-import-boundary` |

---

## Rust (au-delà des 40 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `rust-prefer-cow` | `Cow<'_, T>` pour les données mostly-read | Fonction prenant `&str` qui parfois `.to_owned()` le résultat | Difficile |
| `rust-no-mutex-in-single-threaded` | `Mutex` dans un contexte single-threaded (utiliser `Cell`/`RefCell`) | `Mutex::new(` dans un module sans `spawn` ou async | Difficile |
| `rust-vec-with-capacity` | `Vec::with_capacity()` quand la taille est connue | Boucle `push` sur `Vec::new()` quand la taille source est disponible | Moyen |
| `rust-prefer-channel-over-arc-mutex-vec` | `mpsc::channel` plutôt que `Arc<Mutex<Vec>>` pour les résultats de tâches | `Arc<Mutex<Vec<` avec `.lock().push(` | Moyen |
| `rust-anyhow-context-on-question-mark` | `.context()`/`.with_context()` sur tout `?` en application | `?` sans `.context(` ou `.with_context(` | Moyen |
| `rust-prefer-once-lock` | `OnceLock`/`LazyLock` (std) plutôt que `lazy_static`/`once_cell` | `lazy_static!` macro ou `once_cell::sync::Lazy` | Facile |
| `rust-must-use-on-result-fn` | `#[must_use]` sur les fonctions retournant `Result` dans les libs | `pub fn` retournant `Result<` sans `#[must_use]` dans un crate lib | Moyen |
| `rust-unsafe-ffi-isolation` | Isoler le FFI unsafe dans `mod sys` ou crate `-sys` | `unsafe` avec `extern "C"` en dehors d'un module `sys` | Moyen |
| `rust-thiserror-for-lib` | `thiserror` pour les libs, `anyhow`/`miette` pour les apps | Enum d'erreur sans `#[derive(thiserror::Error)]` dans un crate lib | Moyen |
| `rust-prefer-fast-hasher` | `ahash`/`fxhash` pour les HashMaps à clés entières | `HashMap::<u64,` ou `HashMap::<usize,` sans hasher alternatif | Moyen |

---

## Testing (au-delà des 46 règles existantes)

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `testing-prefer-msw` | MSW pour le mocking réseau, pas `vi.mock('fetch')`/`vi.mock('axios')` | `vi.mock('axios'` ou `global.fetch = vi.fn()` dans les tests | Facile |
| `testing-prefer-test-each` | `test.each` pour les tests paramétriques | ≥ 3 blocs `test(` quasi-identiques (même structure, inputs différents) | Difficile |
| `testing-no-and-in-test-name` | Pas de `and` dans le nom d'un test (split en deux) | `test('... and ...',` | Facile |
| `testing-no-undefined-mock-var` | `vi.hoisted()` pour les variables dans les factories `vi.mock()` | Variable référencée dans `vi.mock(() => ({ ... var ... }))` déclarée hors `vi.hoisted()` | Moyen |
| `testing-no-real-external-service` | Intercepter les services tiers, pas les appeler réellement | `fetch('https://stripe.com'` ou `fetch('https://api.sendgrid.com'` dans les tests | Moyen |

---

## Better Auth

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `better-auth-no-disable-csrf` | `disableCSRFCheck: true` interdit | `disableCSRFCheck: true` dans la config | Facile |
| `better-auth-no-disable-origin-check` | `disableOriginCheck: true` interdit | `disableOriginCheck: true` dans la config | Facile |
| `better-auth-require-secure-cookies` | `useSecureCookies: true` en production | Absence de `useSecureCookies` dans la config advanced | Moyen |
| `better-auth-require-rate-limit` | Rate limiting activé | Absence de `rateLimit: { enabled: true }` dans la config | Moyen |
| `better-auth-plugin-import-path` | Import depuis le chemin dédié pour le tree-shaking | `import { twoFactor } from "better-auth/plugins"` au lieu de `"better-auth/plugins/two-factor"` | Facile |
| `better-auth-trusted-providers` | `trustedProviders` défini pour l'account linking | `account.accountLinking.enabled: true` sans `trustedProviders` | Moyen |
| `better-auth-middleware-requires-headers` | `getSession` en middleware Next.js doit forwarder les headers | `getSession(` dans `middleware.ts` sans `headers: nextHeaders()` | Moyen |

---

## Architecture générale

| ID | Description | Détecte | Faisabilité |
|----|-------------|---------|-------------|
| `no-grab-bag-module` | Pas de modules `common`/`shared`/`utils` fourre-tout | Fichier `common.ts`, `shared.ts`, `utils.ts` contenant plus de 10 exports | Moyen — **proche de** `no-common-grab-bag` (vérifier si existe) |
| `vertical-slice-no-role-folders` | `services/`, `repositories/`, `handlers/` séparés = anti-pattern | Structure `src/services/` + `src/repositories/` + `src/handlers/` en parallèle | Moyen |
| `no-cross-domain-db-access` | Ne pas accéder aux tables DB d'un autre domaine directement | Import de `features/X/schema` depuis `features/Y/` | Moyen — **proche de** `layer-import-boundary` |
| `single-call-site-inline` | Fonction avec un seul call site devrait être inlinée | Fonction exportée référencée dans exactement un seul fichier | Difficile |
| `no-boolean-flag-param` | Pas de paramètre booléen — **déjà couvert** par `no-boolean-flag-param` | **Déjà couvert** | — |

---

## Récapitulatif par volume

| Domaine | Règles nouvelles | Priorité |
|---------|-----------------|----------|
| TanStack Query | 12 | Haute (v5 breaking changes) |
| Vue | 11 | Haute |
| i18n | 7 | Haute |
| Zod | 11 | Haute |
| Sécurité | 10 | Haute |
| React | 10 | Moyenne |
| Tailwind | 7 | Moyenne |
| Rust | 10 | Moyenne |
| TypeScript/Architecture | 7 | Moyenne |
| TanStack Start | 6 | Moyenne |
| Better Auth | 7 | Basse (framework très spécifique) |
| Testing | 5 | Basse |
| Drizzle ORM | 5 | Basse |
| Database SQL | 5 | Basse |
| API Design | 5 | Basse |

**Total : ~118 règles candidates**

---

## À vérifier avant d'implémenter

Ces règles candidates pourraient déjà exister sous un autre nom :
- `no-common-grab-bag` — vérifier si couvre `shared.ts` / `utils.ts`
- `no-new-regex-with-variable` — vérifier si couvre `new RegExp(userInput)` des handlers
- `sql-no-between-timestamp` — la règle SQL existe, vérifier si couvre aussi le Drizzle
- `no-raw-db-entity-in-handler` — vérifier si couvre aussi le mass assignment via `req.body`
