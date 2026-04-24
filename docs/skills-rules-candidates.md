# Skills Rules Candidates

Règles extraites des skills Claude, vérifiées contre les 1120 règles existantes.
Généré le 2026-04-24 par analyse automatique de ~/.claude/skills/.
Doublons retirés après cross-référencement exhaustif.

---

## Better Auth

- [ ] `better-auth-secret-min-length` — BETTER_AUTH_SECRET ou secret config doit faire ≥ 32 caractères — **TS**
  - REVIEW [FIX] L'implémentation ne contrôle que les littéraux `secret: "..."`. Le cas explicitement mentionné par la règle, `process.env.BETTER_AUTH_SECRET` / variable d'env courte, n'est pas couvert.
- [ ] `better-auth-no-duplicate-baseurl` — Ne pas set baseURL dans betterAuth() quand BETTER_AUTH_URL env est défini — **TS**
  - REVIEW [FIX] La règle est conditionnelle à la présence de `BETTER_AUTH_URL`, mais l'implémentation signale tout `baseURL` dans `betterAuth(...)`. Un projet sans env var dédiée aura un faux positif.
- [ ] `better-auth-no-duplicate-secret` — Ne pas set secret dans betterAuth() quand BETTER_AUTH_SECRET env est défini — **TS**
  - REVIEW [FIX] Même problème que `baseURL` : l'implémentation signale tout `secret` dans `betterAuth(...)` sans prouver que `BETTER_AUTH_SECRET` est défini.
- [ ] `better-auth-drizzle-useplural` — Quand drizzleAdapter utilise une table nommée `users` (pluriel), exiger usePlural: true — **TS**
  - REVIEW [FIX] La détection repose sur `obj_text.contains("users")`. Un commentaire, une string ou une propriété non-table peut déclencher la règle, et l'implémentation ne vérifie pas réellement une table `users` dans le schema Drizzle.
- [ ] `better-auth-expo-no-cookie-auth` — En React Native/Expo, interdire l'auth par cookies ; exiger @better-auth/expo expoClient() — **TSX**
  - REVIEW [FIX] N'importe quelle occurrence textuelle de `expoClient` supprime le diagnostic, même un commentaire ou un import inutilisé. Il faut vérifier que `expoClient()` est bien présent dans `plugins`.
- [ ] `better-auth-session-infer-type` — Utiliser typeof auth.$Infer.Session plutôt qu'une interface session manuelle — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-auth-email-verification-handler` — Si emailVerification.sendOnSignUp: true, exiger sendVerificationEmail handler défini — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-auth-reset-password-handler` — Si emailAndPassword.enabled et reset flow, exiger sendResetPassword handler — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-auth-client-framework-import` — Importer le client depuis le path spécifique au framework (better-auth/react, /vue, etc.) — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-auth-required-user-fields` — Le schema user doit inclure les champs email et name — **TS**
  - REVIEW [FIX] La présence de `email` / `name` est testée par substring dans tout l'objet `user`. Un commentaire ou un champ imbriqué sans rapport peut faire passer une config où les champs requis manquent.

## Better Result

- [ ] `better-result-no-throw` — Dans les modules important better-result, throw est interdit pour les erreurs domaine/infra ; retourner Result.err() — **TS**
  - REVIEW [FIX] Cette règle signale aussi les `throw` placés dans le callback de `Result.try` / `Result.tryPromise`, alors que c'est précisément le pattern de migration attendu par les autres règles.
- [ ] `better-result-no-try-catch` — Remplacer try/catch par Result.try({ try, catch }) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-no-promise-catch` — Remplacer .catch() sur Promise par Result.tryPromise({ try, catch }) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-no-nullable-return` — Les fonctions retournant T | null/undefined pour "not found" doivent retourner Result<T, NotFoundError> — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-no-manual-propagation` — Interdire if (r.isErr()) return Result.err(r.error) ; utiliser Result.gen + yield* — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-no-rewrap-error` — Interdire return Result.err(result.error) quand return result suffit — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-require-gen-for-chains` — Quand 2+ Result sont chaînés, exiger Result.gen + yield* plutôt que .andThen imbriqués — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-prefer-map-single` — Interdire Result.gen wrappant une seule transformation ; utiliser .map()/.andThen() — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-tagged-error-message` — Les classes étendant TaggedError doivent déclarer un champ message: string — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-tagged-error-cause-unknown` — Le champ cause dans TaggedError doit être typé unknown, pas Error/any — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-tag-matches-classname` — Le string passé à TaggedError("X") doit correspondre au nom de la classe X — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-no-param-properties` — Les constructeurs TaggedError doivent appeler super({ ...args, message }), pas de parameter properties — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-try-requires-catch` — Result.try/tryPromise doit inclure à la fois try et catch — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-catch-returns-tagged` — Le catch de Result.tryPromise doit retourner un TaggedError, pas un Error/string brut — **TS**
  - REVIEW [FIX] L'implémentation ne détecte que `new Error(` dans le handler `catch`. Les retours `Error(...)`, string, objet brut ou variable `err` passent alors que la règle les interdit.
- [ ] `better-result-no-catch-panic` — Interdire les catch qui matchent/re-handle Panic de better-result — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-await-inside-gen` — Dans Result.gen, les Promise-returning Result doivent utiliser yield* Result.await() — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-no-mixed-throw` — Une fonction retournant Result<...> ne doit pas contenir de throw (sauf dans Result.try) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `better-result-caller-must-handle` — Les Result retournés ne doivent pas être ignorés ; exiger yield*/match/map/unwrap/assignment — **TS**
  - REVIEW [FIX] La règle ne reconnaît que les appels `Result.*` ou les fonctions dont le nom finit par `Result`. Un appel ignoré comme `findUser(id);` retournant `Result<T,E>` n'est pas détecté.
- [ ] `better-result-prefer-matcherror-exhaustive` — Préférer matchError à matchErrorPartial quand l'union est complètement énumérable — **TS**
  - REVIEW [FIX] L'implémentation signale tous les `matchErrorPartial`, sans vérifier la condition "quand l'union est complètement énumérable". Les unions ouvertes ou inconnues deviennent des faux positifs.
- [ ] `better-result-constructor-spreads-args` — Les constructeurs TaggedError avec message calculé doivent spread args dans super() — **TS**
  - REVIEW [TODO] Reste à reviewer.

## API Design

- [ ] `api-separate-input-output-types` — Le même type ne doit pas servir à la fois pour le request input et le response output (id, createdAt ne sont pas dans l'input) — **TS**
  - REVIEW [FIX] L'implémentation ne vérifie pas qu'un même type est réellement utilisé en input et en output ; elle signale des types "bare entity" avec champs serveur comme proxy. Des entités internes légitimes peuvent donc être signalées.
- [ ] `api-no-internal-ids-in-response` — Les DTOs de réponse ne doivent pas exposer les noms de colonnes internes, IDs séquentiels ou champs d'implémentation — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `api-branded-id-types` — Les IDs d'entités dans les signatures API publiques doivent utiliser des branded types (OrderId), pas string/number brut — **TS**
  - REVIEW [FIX] La portée "signatures API publiques" n'est pas respectée : tout paramètre `id` / `orderId` typé `string` ou `number` est signalé, y compris dans des helpers internes.
- [ ] `api-no-nullable-variant-fields` — Interdire les champs optionnels conditionnels ("existe seulement quand status=X") ; exiger des discriminated unions — **TS**
  - REVIEW [FIX] La règle ne détecte que 3 champs optionnels ou plus partageant un préfixe. Le cas courant `status: "cancelled"; cancelReason?: string` ou 1-2 champs conditionnels passe.
- [ ] `api-put-vs-patch` — Les handlers PUT qui font des partial updates (sémantique champs-fournis-seulement) doivent être PATCH — **TS**
  - REVIEW [FIX] Le diagnostic repose uniquement sur la présence de `Partial<...>` dans un appel `.put(...)`. Cela ne prouve pas la sémantique "champs fournis seulement" et peut signaler des wrappers/types auxiliaires sans partial update réel.
- [ ] `api-validate-at-boundaries` — Interdire les validation schemas (zod.parse) entre fonctions internes partageant un contrat typé ; validation aux frontières uniquement — **TS**
  - REVIEW [FIX] Le heuristique autorise toute fonction nommée avec un préfixe HTTP comme `getUser` ou `postProcess`. Des services internes avec `Schema.parse(...)` peuvent donc échapper à la règle.

## Database / SQL

- [ ] `sql-no-uuidv4-primary-key` — Les colonnes PK UUID avec gen_random_uuid()/uuid_generate_v4() doivent utiliser UUIDv7 ou BIGINT IDENTITY — **SQL**
- [ ] `sql-singular-table-names` — Les noms de table dans CREATE TABLE doivent être au singulier — **SQL**
- [ ] `sql-boolean-column-prefix` — Les colonnes BOOLEAN doivent commencer par is_ ou has_ — **SQL**
- [ ] `sql-no-is-deleted-boolean` — Interdire is_deleted BOOLEAN ; exiger deleted_at TIMESTAMPTZ — **SQL**
- [ ] `sql-constraint-naming-convention` — Les contraintes doivent suivre {table}_{col}_{suffix} avec suffix pk|fk|key|chk|exl|idx — **SQL**
- [ ] `sql-fk-naming-convention` — Les foreign keys doivent être nommées {from_table}_{from_col}_{to_table}_{to_col}_fk — **SQL**
- [ ] `sql-no-now-in-transaction` — Interdire NOW() dans les blocs BEGIN/transaction ; utiliser clock_timestamp() — **SQL**
- [ ] `sql-no-truncate-in-app` — Interdire TRUNCATE dans les migrations/queries applicatives ; exiger DELETE FROM — **SQL**
- [ ] `sql-no-reserved-keyword-identifiers` — Interdire les mots réservés PostgreSQL comme noms de table/colonne — **SQL**
- [ ] `sql-no-function-on-indexed-column` — Interdire WHERE fn(col) = ... (date_trunc, LOWER, UPPER) qui tue la SARGabilité — **SQL**
- [ ] `sql-no-select-then-insert-race` — Interdire SELECT+INSERT séquentiel sur la même clé ; exiger ON CONFLICT — **SQL**
- [ ] `sql-add-constraint-not-valid` — ALTER TABLE ADD CONSTRAINT doit utiliser NOT VALID suivi d'un VALIDATE séparé — **SQL**
- [ ] `sql-no-disable-autovacuum` — Interdire autovacuum_enabled = false — **SQL**
- [ ] `sql-require-search-path` — Les fichiers de migration doivent SET search_path = pg_catalog ou utiliser des noms qualifiés — **SQL**
- [ ] `sql-no-union-when-union-all` — Interdire UNION quand les colonnes incluent un PK garantissant l'unicité ; utiliser UNION ALL — **SQL**
- [ ] `sql-no-rename-column` — Interdire ALTER TABLE RENAME COLUMN ; exiger expand-contract — **SQL**
- [ ] `sql-no-drop-column-without-expand` — Interdire DROP COLUMN sans migration préalable marquant la colonne unused — **SQL**

## Drizzle ORM

- [ ] `drizzle-json-requires-type` — json()/jsonb() sans .$type<T>() doit être signalé (pas de JSON unknown/any) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-prefer-infer-select` — Utiliser typeof table.$inferSelect plutôt que InferSelectModel<typeof x> — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-camel-snake-column-names` — Propriété TS en camelCase, arg string des constructeurs de colonnes en snake_case — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-created-at-default-now` — Une colonne createdAt/created_at de type timestamp doit avoir .defaultNow() — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-updated-at-on-update` — Une colonne updatedAt/updated_at doit avoir .$onUpdate(() => new Date()) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-config-satisfies` — Dans drizzle.config.ts, exiger } satisfies Config plutôt que const config: Config — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-junction-composite-pk` — Une table junction (seulement 2 FK IDs) doit déclarer primaryKey({ columns: [...] }) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-prefer-findmany-relations` — Signaler les .leftJoin/.innerJoin manuels quand des relations() existent ; utiliser db.query.X.findMany({ with }) — **TS**
  - REVIEW [FIX] L'implémentation signale tous les joins manuels, sans vérifier que des `relations()` existent pour les tables concernées. La condition centrale de la règle n'est pas prouvée.
- [ ] `drizzle-multi-statement-tx` — Les db.insert/update/delete séquentiels sur tables liées sans db.transaction() doivent être signalés — **TS**
  - REVIEW [FIX] La détection ne compte que les mutations en `expression_statement` au niveau direct du bloc. Des mutations séquentielles dans des branches, callbacks ou helpers appelés dans le même flux ne seront pas vues.
- [ ] `drizzle-pool-requires-timeouts` — new Pool() passé à drizzle() doit définir idleTimeoutMillis et connectionTimeoutMillis — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-serverless-pool-max-one` — En serverless (Edge/Lambda), new Pool() avec drizzle doit avoir max: 1 — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-no-new-pool-per-request` — Interdire new Pool()/drizzle() dans le corps d'un handler exporté ; doit être module-scope — **TS**
  - REVIEW [FIX] La règle annoncée cible les handlers exportés, mais l'implémentation signale `new Pool()` / `drizzle()` dans n'importe quelle fonction. Une factory interne appelée au démarrage peut être faussement signalée.
- [ ] `drizzle-prefer-inarray` — Signaler sql`... IN (...)` quand inArray(col, [...]) pourrait être utilisé — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-soft-delete-filter` — Dans les modules avec colonne deletedAt, les select/findMany doivent inclure isNull(t.deletedAt) — **TS**
  - REVIEW [FIX] Le déclencheur est `ctx.source.contains("deletedAt")`, puis tous les `select/findMany` du fichier sont contrôlés. Une requête sur une table non soft-deletable dans le même fichier sera signalée à tort.
- [ ] `drizzle-prepared-placeholder` — Dans les chaînes .prepare(), les clauses where doivent utiliser sql.placeholder() plutôt que des variables inline — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-consistent-table-naming` — Le premier arg de pgTable/mysqlTable/sqliteTable doit être snake_case lowercase plural — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `drizzle-zod-omit-generated` — createInsertSchema(table) pour validation API doit .omit({ id, createdAt, ... }) des colonnes auto-générées — **TS**
  - REVIEW [FIX] L'implémentation vérifie seulement la présence de `.omit(`. Un `createInsertSchema(users).omit({ name: true })` passerait même si `id` / `createdAt` restent acceptés.

## TanStack Query

- [ ] `tanstack-query-object-syntax` — Interdire les appels positionnels useQuery(key, fn, opts) ; exiger la syntaxe objet useQuery({ queryKey, queryFn }) — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-serializable-key` — Interdire les valeurs non-sérialisables dans queryKey : fonctions, new Date(), Symbol(), instances de classe — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-infinite-initial-page-param` — useInfiniteQuery/infiniteQueryOptions doit inclure initialPageParam — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-pass-signal-to-fetch` — Quand queryFn destructure { signal }, ce signal doit être passé à fetch(..., { signal }) — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-dependent-needs-enabled` — Si queryFn utilise une valeur pouvant être undefined/nullable, exiger enabled: !!value — **TS/TSX**
  - REVIEW [FIX] La détection se limite à `?.` et `!` dans `queryFn`. Un `queryFn: () => fetchUser(userId)` avec `userId: string | undefined` n'est pas couvert.
- [ ] `tanstack-query-invalidate-after-mutation` — useMutation avec fetch write (POST/PATCH/PUT/DELETE) doit invalidateQueries ou setQueryData dans onSuccess/onSettled — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-no-global-onerror-v5` — Interdire defaultOptions.queries.onError dans new QueryClient ; utiliser QueryCache({ onError }) — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-test-retry-false` — Dans les fichiers test, new QueryClient doit set defaultOptions.queries.retry: false — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-no-v4-import-path` — Interdire imports depuis 'react-query' ; exiger '@tanstack/react-query' — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-no-enabled-on-suspense` — Interdire enabled sur useSuspenseQuery — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-max-pages-requires-both` — Quand maxPages est set sur une infinite query, getNextPageParam ET getPreviousPageParam doivent être définis — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-query-no-mutation-for-client-state` — Interdire useMutation/useQuery pour du state purement local sans interaction réseau — **TS/TSX**
  - REVIEW [FIX] Toute présence de `await` est traitée comme une interaction réseau. Une mutation purement locale mais async, par exemple `await sleep(0); setOpen(false)`, passe alors que la règle l'interdit.

## TanStack Start

- [ ] `tanstack-start-session-cookie-httponly` — useSession({ cookie }) doit set httpOnly: true — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-start-session-cookie-samesite` — useSession({ cookie }) doit set sameSite 'lax' ou 'strict' — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-start-session-cookie-secure` — useSession({ cookie }) doit set secure (gateable sur NODE_ENV) — **TS**
  - REVIEW [FIX] L'implémentation accepte n'importe quelle clé `secure`, y compris `secure: false`. La règle demande que le cookie soit effectivement sécurisé, éventuellement avec un gate `NODE_ENV`.
- [ ] `tanstack-start-session-secret-min-length` — useSession({ password }) doit référencer une env var, pas un literal ; si literal, ≥ 32 chars — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-start-server-fn-post-for-mutations` — Les server functions nommées create/update/delete/login/logout doivent utiliser method: 'POST' — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-start-server-fn-use-notfound` — Dans les handlers createServerFn, throw notFound() plutôt que throw new Error('not found') — **TS**
  - REVIEW [FIX] La portée `createServerFn` n'est pas vérifiée : tout `throw new Error("not found")` du fichier est signalé, même dans du code hors TanStack Start.
- [ ] `tanstack-start-route-protection-beforeload` — Les routes protégées doivent utiliser beforeLoad + throw redirect(), pas useEffect + navigate — **TS/TSX**
  - REVIEW [FIX] La règle ne détecte que `useEffect` qui navigue vers `/login`. Les redirections auth vers d'autres routes ou les gardes protégés sans `/login` ne sont pas couverts.
- [ ] `tanstack-start-no-date-now-in-render` — Interdire Date.now()/new Date()/Math.random() dans le corps render des route components (hydration mismatch) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tanstack-start-no-window-in-render` — Interdire window.*/document.* dans le top-level render (hors useEffect/typeof guard) — **TSX**
  - REVIEW [FIX] Le texte de la règle autorise un `typeof window !== "undefined"` guard, mais l'implémentation ne le reconnaît pas et signale quand même l'accès dans le render.
- [ ] `tanstack-start-no-fetch-to-own-api` — Interdire fetch('/api/...') quand un createServerFn équivalent existe — **TS/TSX**
  - REVIEW [FIX] L'implémentation signale tout `fetch("/api/...")` sans vérifier qu'un `createServerFn` équivalent existe. La condition qui limite la règle n'est pas implémentée.
- [ ] `tanstack-start-api-route-json-helper` — Les handlers de route API doivent utiliser json() de @tanstack/react-start, pas new Response(JSON.stringify()) — **TS**
  - REVIEW [TODO] Reste à reviewer.

## Vue

- [ ] `vue-ref-value-in-script` — Signaler la lecture d'un ref() dans <script> sans .value dans les conditions/comparaisons — **Vue SFC**
- [ ] `vue-no-value-on-reactive` — Interdire .value sur une variable produite par reactive() — **Vue SFC**
- [ ] `vue-shallowref-for-primitives` — Signaler ref(<primitive literal>) quand shallowRef suffirait — **Vue SFC**
- [ ] `vue-no-watch-reactive-property` — Signaler watch(state.prop, ...) (valeur non-getter) ; exiger la forme getter ou toRefs — **Vue SFC**
- [ ] `vue-watch-immediate-over-onmounted` — Signaler onMounted(() => fn(x.value)) couplé avec watch(x, fn) ; exiger { immediate: true } — **Vue SFC**
- [ ] `vue-no-v-if-with-v-for` — Interdire v-if et v-for sur le même élément — **Vue template**
- [ ] `vue-computed-no-side-effects` — Interdire emit(), console.*, appels API, mutations et assignments dans un computed() — **Vue SFC**
- [ ] `vue-scoped-styles-preferred` — Signaler les blocs <style> non-scoped dans les SFC composants (les styles globaux doivent vivre ailleurs) — **Vue SFC**
- [ ] `vue-use-template-ref` — En Vue 3.5+, signaler const x = ref(null) utilisé comme template ref ; exiger useTemplateRef('x') — **Vue SFC**
- [ ] `vue-define-model-over-modelvalue` — En Vue 3.4+, signaler defineProps<{ modelValue }> + update:modelValue ; exiger defineModel — **Vue SFC**
- [ ] `vue-inject-key-typed` — Signaler les clés string dans provide()/inject() ; exiger des symboles InjectionKey<T> — **Vue SFC**
- [ ] `vue-typed-define-props-emits` — En lang="ts", exiger la forme type defineProps<...>() / defineEmits<...>() plutôt que la forme runtime objet — **Vue SFC**
- [ ] `vue-custom-directive-v-prefix` — Les directives locales dans <script setup> doivent être nommées vXxx — **Vue SFC**
- [ ] `vue-setup-store-return-all` — Pinia setup stores : chaque ref/reactive/computed déclaré doit apparaître dans l'objet retourné — **Vue/TS**
- [ ] `vue-no-usestore-top-level` — Dans un store Pinia, interdire useOtherStore() au top-level du setup ; doit être dans une action/getter — **Vue/TS**
- [ ] `vue-no-ssr-globals-in-setup` — Interdire window, document, localStorage, navigator au top-level de <script setup> ; exiger onMounted — **Vue SFC**
- [ ] `vue-withdefaults-factory` — Les valeurs par défaut array/object dans withDefaults doivent être des factory functions (() => []) — **Vue SFC**
- [ ] `vue-v-memo-requires-v-for` — v-memo doit être sur un élément qui a aussi v-for (ou v-memo="[]" pour subtree statique) — **Vue template**
- [ ] `vue-no-filter-sort-in-template` — Interdire .filter()/.sort()/appels de fonction retournant des arrays directement dans les expressions v-for — **Vue template**

## Zod (v4+)

- [ ] `zod-record-two-args` — Signaler z.record(valueSchema) à un seul arg ; exiger z.record(keySchema, valueSchema) (v4) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-prefer-stringbool` — Signaler z.coerce.boolean() sur les inputs de formulaire HTML ; préférer z.stringbool() (v4) — **TS**
  - REVIEW [FIX] La règle est limitée aux inputs HTML/form/query, mais l'implémentation signale tous les `z.coerce.boolean()` quel que soit le contexte.
- [ ] `zod-prefer-strict-object` — Signaler z.object({}).strict() ; préférer z.strictObject({}) (v4) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-prefer-loose-object` — Signaler z.object({}).passthrough() ; préférer z.looseObject({}) (v4) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-prefer-extend-over-merge` — Signaler .merge() ; préférer .extend() (v4) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-no-manual-types` — Signaler les types manuels dupliquant un schema Zod ; exiger z.infer<typeof schema> — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-require-input-for-transforms` — Signaler z.infer sur un schema avec .transform() pour les types d'input de form ; exiger z.input — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-no-coerce-on-financial` — Interdire z.coerce.* sur les champs money/price/amount/currency ; exiger transform + validation explicite — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-require-multipleof-currency` — Les champs number pour monnaie doivent avoir .multipleOf(0.01) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `zod-no-schema-in-hot-path` — Interdire z.object()/z.string() dans un render React, corps de boucle ou request handler ; doit être module-level — **TS/TSX**
  - REVIEW [FIX] Les corps de boucle mentionnés dans la règle ne sont pas contrôlés. L'implémentation ne cherche que des composants React ou handlers request.
- [ ] `zod-prefer-overwrite-v4` — Signaler .transform() dont le type de sortie = type d'entrée ; préférer .overwrite() (v4) — **TS**
  - REVIEW [FIX] La détection ne couvre que quelques formes textuelles (`param.method(...)`, `Math.*(param)`). Beaucoup de transforms entrée=sortie, par exemple normalisation conditionnelle ou helper pur, ne seront pas signalés.

## TypeScript

- [ ] `ts-no-as-narrowing` — Interdire as pour narrower des types ; exiger des type predicates ou checks in — **TS**
  - REVIEW [FIX] L'implémentation ne signale que les casts vers literal/template literal types. Les casts narrowing courants comme `value as AdminUser` ou `value as NonNullable<T>` passent.
- [ ] `ts-no-narrowing-across-closures` — Signaler les variables narrowed utilisées dans setTimeout/.then/event handlers sans capture const — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-no-mixed-sync-async-returns` — Interdire les fonctions retournant conditionnellement T ou Promise<T> ; exiger async partout — **TS**
  - REVIEW [FIX] Seules les annotations explicites `T | Promise<T>` sont détectées. Une fonction non annotée avec `return value` dans une branche et `return fetchValue()` dans une autre n'est pas couverte.
- [ ] `ts-no-generic-return-only` — Interdire les paramètres génériques qui n'apparaissent qu'en return position (pas de site d'inférence) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-prefer-interface-extends` — Préférer interface X extends A, B plutôt que type X = A & B pour la composition d'object types — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-no-large-string-union` — Signaler les unions de string literals dépassant un seuil configurable (ex: >50 membres) — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-bounded-recursive-generic` — Exiger un accumulateur de profondeur sur les types conditionnels/mapped récursifs — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-require-variance-annotation` — Exiger les annotations in/out sur les paramètres génériques des interfaces exportées — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-no-unused-generic-parameter` — Signaler les paramètres génériques non référencés dans les paramètres ou le type de retour — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-assertion-fn-must-be-declaration` — Interdire les arrow functions pour les asserts type predicates ; exiger function declaration — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-branded-type-no-direct-cast` — Interdire as BrandedType en dehors des fonctions validator dédiées — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-overload-signature-order` — Exiger les signatures d'overload ordonnées du plus spécifique au plus général — **TS**
  - REVIEW [FIX] L'ordre est évalué seulement via le nombre de paramètres requis. Deux overloads avec le même arity mais des types plus ou moins spécifiques (`"a"` avant `string`) ne sont pas vérifiés.
- [ ] `ts-declare-global-requires-export` — Exiger export {} dans les fichiers contenant declare global — **TS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ts-no-mixed-decorator-systems` — Interdire le mélange de decorators standard et experimentalDecorators — **TS**
  - REVIEW [TODO] Reste à reviewer.

## Rust

- [ ] `rust-workspace-deps-centralized` — Dans un workspace Cargo, les crates membres doivent utiliser { workspace = true } plutôt que pinning individuel — **Rust**
- [ ] `rust-workspace-lints-shared` — Les workspace projets doivent définir [workspace.lints] et les membres hériter via [lints] workspace = true — **Rust**
- [ ] `rust-no-arc-mutex-tree` — Signaler Arc<Mutex<Node>>/Rc<RefCell<Node>> dans les structures arbre/graphe ; recommander les arena allocators — **Rust**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rust-no-println-in-async` — Interdire println!/eprintln! dans le code async ; exiger les macros tracing — **Rust**
  - REVIEW [FIX] La règle vise le code async, mais l'implémentation s'appuie sur `is_inside_async_fn`. Les `async { println!(...) }` ou callbacks async hors `async fn` risquent de passer.
- [ ] `rust-asref-path-for-fs-fns` — Les fonctions filesystem doivent accepter impl AsRef<Path> plutôt que &Path/&str/PathBuf — **Rust**
  - REVIEW [TODO] Reste à reviewer.

## Security

- [ ] `security-bcrypt-min-rounds` — bcrypt.hash(..., n) avec n < 12 doit être signalé — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `security-require-helmet` — Les apps Express (express()) sans app.use(helmet()) doivent être signalées — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `security-require-rate-limit-auth` — Les handlers de route auth (/login, /signup, /reset) sans middleware de rate limit doivent être signalés — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `security-require-pkce-oauth` — La construction d'URL OAuth authorize sans code_challenge doit être signalée — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `security-require-oauth-state` — Les handlers de callback OAuth qui ne lisent/valident pas le param state doivent être signalés — **TS/JS**
  - REVIEW [FIX] L'implémentation accepte toute occurrence textuelle de `state` dans le handler. Une simple lecture, un commentaire ou une variable non comparée peut masquer l'absence de validation CSRF.
- [ ] `security-no-deserialize-untrusted` — Signaler pickle.loads, yaml.load (unsafe), node-serialize.unserialize avec input utilisateur — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `security-no-query-without-ownership` — Signaler SELECT/findById sans filtre userId/orgId dans les handlers de route (risque IDOR) — **TS/JS**
  - REVIEW [FIX] La portée "handlers de route" n'est pas appliquée : tout `findById` / `findUnique` sans `userId`/`orgId` est signalé, y compris dans des scripts admin ou jobs internes.
- [ ] `security-require-hsts` — Les apps Express/HTTP sans header HSTS doivent être signalées — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `security-no-sri-missing` — Les tags <script src="https://..."> sans attribut integrity= doivent être signalés — **TSX/HTML**
  - REVIEW [TODO] Reste à reviewer.

## Web Performance

- [ ] `perf-img-fetchpriority-high` — L'image hero/LCP doit avoir fetchpriority="high" (et pas loading="lazy") — **TSX/HTML**
  - REVIEW [FIX] Les dimensions JSX numériques (`width={1200}`, `height={800}`) ne sont pas prises en compte, seulement les attributs string. Une image hero courante peut donc passer sans `fetchpriority`.
- [ ] `perf-img-modern-format` — Signaler <img src> en .jpg/.png sans fallback WebP/AVIF via <picture>/srcset — **TSX/HTML**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `perf-font-face-display-swap` — Chaque règle @font-face doit inclure font-display: swap — **CSS**
- [ ] `perf-font-preload-crossorigin` — <link rel="preload" as="font"> doit inclure crossorigin et type="font/woff2" — **TSX/HTML**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `perf-no-google-fonts-link` — Signaler <link href="fonts.googleapis.com"> ; préférer le self-hosting — **TSX/HTML**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `perf-route-level-code-split` — Les composants de route doivent être importés via React.lazy/dynamic import, pas en import statique — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `perf-prefers-reduced-motion` — Les CSS avec animation/@keyframes doivent inclure un bloc @media (prefers-reduced-motion: reduce) — **CSS**
- [ ] `perf-no-render-blocking-css` — Les <link rel="stylesheet"> dans <head> pour CSS non-critique doivent avoir un media attribute — **TSX/HTML**
  - REVIEW [TODO] Reste à reviewer.

## i18n

- [ ] `i18n-no-english-key` — Signaler t("Full sentence...") où la clé contient des espaces ou commence par une majuscule ; la clé doit être un identifiant domain.key — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `i18n-key-requires-domain-prefix` — Les clés t() doivent contenir au moins un . (préfixe domaine) et matcher le pattern ^[a-z]...(\....)+ — **TS/TSX**
  - REVIEW [FIX] L'implémentation vérifie seulement la présence d'un point. Des clés invalides au regard du pattern (`Auth.Title`, `auth..title`, `auth/title`) peuvent passer selon le cas.
- [ ] `i18n-max-key-depth` — Signaler les clés t() avec plus de 2 points (plus de 2 niveaux de nesting) — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `i18n-use-singleton-outside-react` — Signaler useTranslation() dans head(), Zod error maps, QueryCache handlers ; exiger i18n.t() — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `i18n-no-manual-list-join` — Signaler array.join(", ")/join(" and ") sur des arrays user-visible ; exiger Intl.ListFormat — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `i18n-key-exists` — Signaler t("key") / i18n.t("key") où la clé string literal n'existe pas dans les fichiers locale — **TS/TSX (cross-file)**
  - REVIEW [FIX] La règle annoncée est cross-file, mais l'implémentation ne lit pas les fichiers locale. Elle détecte seulement les clés malformées (`..`, point initial/final), pas les clés absentes.

## React

- [ ] `react-no-use-client-without-client-api` — Signaler "use client" dans les fichiers qui n'utilisent ni hooks, ni event handlers, ni browser APIs — **TSX**
  - REVIEW [FIX] La détection compte les identifiers dans les imports. Un fichier qui importe `useState` sans l'utiliser est considéré comme utilisant une API client et ne sera pas signalé.
- [ ] `react-no-barrel-import-known-libs` — Signaler les imports nommés depuis lucide-react, @mui/material, @mui/icons-material, react-icons, lodash, date-fns ; exiger les subpath imports — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-destructure-zustand-store` — Signaler const { x, y } = useStore() ; exiger la forme sélecteur useStore(s => s.x) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-setstate-without-updater` — Signaler setX(x + ...) / setX([...x, ...]) ; exiger setX(prev => ...) quand basé sur la valeur précédente — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-find-in-map-loop` — Signaler .find()/.filter() imbriqués dans .map() ou des boucles for itérant un autre array ; utiliser Map — **TS/TSX**
  - REVIEW [FIX] L'implémentation ne vérifie pas que `.find()` / `.filter()` parcourt un autre array. Elle signale toute recherche dans un `.map` ou une boucle, même si ce n'est pas le cas O(n²) visé.
- [ ] `react-no-chained-filter-map-reduce` — Signaler 3+ .filter/.map/.reduce chaînés sur le même array — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-sort-for-extrema` — Signaler arr.sort(...)[0] ou sorted[sorted.length-1] pour obtenir un min/max — **TS/TSX**
  - REVIEW [FIX] Le cas `sorted[sorted.length - 1]` annoncé n'est pas couvert : l'implémentation ne suit pas l'identifiant initialisé par `arr.sort(...)`, elle ne détecte que l'indexation directe du call `sort()`.
- [ ] `react-no-interleaved-layout-rw` — Signaler lectures de offsetWidth/getBoundingClientRect intercalées avec écritures .style.* (layout thrashing) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-unwrapped-localstorage` — Signaler localStorage.getItem/setItem hors d'un try/catch — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-require-versioned-storage-key` — Signaler localStorage.setItem avec une clé literal sans suffixe de version (:vN) — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-usestate-high-frequency` — Signaler useState mis à jour dans des handlers mousemove/scroll/resize/pointermove ; utiliser useRef — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-boolean-variant-props` — Signaler 2+ props booléennes de "mode" (isPrimary, isGhost) sur un composant ; exiger un enum variant — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-dedup-filter-indexof` — Signaler arr.filter((v,i,a) => a.indexOf(v) === i) ; utiliser [...new Set(arr)] — **TS/TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `react-no-blocking-log-after-mutation` — Signaler await log()/analytics après la mutation principale dans les server actions ; utiliser after() — **TS**
  - REVIEW [FIX] La règle ne s'applique qu'aux `export async function`. Les server actions exportées sous forme `export const action = async (...) => ...` ne sont pas couvertes.
- [ ] `react-require-content-visibility` — Signaler .map() rendant 20+ items sans content-visibility: auto ni virtualisation — **TSX**
  - REVIEW [FIX] La règle ne détecte que les arrays littéraux ou `Array.from({ length: N })`. Le cas réel le plus fréquent, `items.map(...)` avec une collection potentiellement longue, passe.

## Tailwind

- [ ] `tailwind-no-legacy-directives` — Interdire @tailwind base/components/utilities ; exiger @import "tailwindcss" en v4 — **CSS**
- [ ] `tailwind-no-tailwindcss-animate` — Interdire l'import/utilisation du package tailwindcss-animate ; utiliser tw-animate-css — **CSS/TS**
  - REVIEW [FIX] La règle est annoncée **CSS/TS**, mais l'implémentation ne contrôle que les imports/require TS. Les usages dans config CSS/Tailwind ne seront pas signalés.
- [ ] `tailwind-no-raw-color-utilities` — Interdire bg-white, text-gray-900, bg-blue-500 dans les composants ; exiger les tokens sémantiques — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-no-manual-dark-variants` — Interdire dark: variants couplées avec des couleurs raw quand les tokens sémantiques existent — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-no-transition-all-layout` — Interdire transition-all combiné avec width/height/top/left ; exiger transition-transform/opacity — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-require-focus-ring` — Exiger focus:ring-2 sur button, a, input, select, textarea, et role=button — **TSX**
  - REVIEW [FIX] `focus:outline-none` est accepté car la fonction cherche tout préfixe `focus:outline-`. C'est pourtant l'inverse d'un indicateur de focus visible.
- [ ] `tailwind-require-motion-reduce` — Exiger motion-reduce:transition-none sur les éléments avec transition-*/animate-* — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-min-touch-target` — Signaler les éléments interactifs plus petits que ~44x44px (ex: px-2 py-1 text-xs sur button) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-require-responsive-text` — Signaler les headings text-4xl+ sans variants responsive (sm:/md:/lg:) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-require-responsive-grid` — Signaler les grid multi-colonnes (grid-cols-2+) sans fallback mobile (grid-cols-1 md:grid-cols-N) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `tailwind-no-off-scale-spacing` — Signaler les valeurs de spacing impaires hors de l'échelle 4/6/8/12/16/24 (ex: p-5, mb-7) — **TSX**
  - REVIEW [TODO] Reste à reviewer.

## Coding Standards

- [ ] `comment-max-words` — Signaler les commentaires dont les phrases dépassent 10 mots — **TS/JS/Rust**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `no-history-in-comments` — Signaler les commentaires contenant "was", "previously", "refactored", "rewritten" — **TS/JS/Rust**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `function-doc-banned-verbs` — Signaler les docstrings de fonctions commençant par reads/pulls/fetches/loads/sums/counts/aggregates/iterates — **TS/JS/Rust**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `no-shallow-passthrough-method` — Signaler les méthodes qui forwarded vers une autre avec une signature identique sans logique ajoutée — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `law-of-demeter-max-dots` — Signaler les accès chaînés 2+ niveaux de profondeur sur une dépendance (a.b().c()) — **TS/JS**
  - REVIEW [FIX] La règle documente un seuil à 2 niveaux avec l'exemple `a.b().c()`, mais l'implémentation ne signale que les chaînes de profondeur strictement supérieure à 2. L'exemple donné peut passer.

## Testing

- [ ] `testing-no-mocking-internal-modules` — Signaler vi.mock('./...') / jest.mock('./...') de paths relatifs internes ; mocker uniquement les frontières — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `testing-no-concurrent-without-context-expect` — Signaler test.concurrent utilisant l'expect module-level au lieu du { expect } destructuré du context — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `testing-require-testid-kebab-case` — Signaler les valeurs d'attribut data-test/data-testid qui ne sont pas en kebab-case — **TSX/HTML**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `testing-no-shared-state` — Signaler les let/var top-level dans les fichiers test mutés à travers les blocs test() sans reset beforeEach — **TS/JS**
  - REVIEW [FIX] Seules les réassignations directes `x = ...` sont détectées. Les mutations de state partagé comme `items.push(...)`, `cache.set(...)` ou `fixture.value = ...` passent.
- [ ] `testing-no-mocktimers-without-restore` — Signaler vi.useFakeTimers() sans vi.useRealTimers() dans afterEach/afterAll — **TS/JS**
  - REVIEW [FIX] La présence de `useRealTimers` n'importe où dans le fichier suffit. La règle demande explicitement un restore dans `afterEach` / `afterAll`, donc un appel dans un test ou helper peut masquer la fuite.
- [ ] `testing-no-stubglobal-without-restore` — Signaler vi.stubGlobal()/vi.stubEnv() sans unstubAllGlobals/unstubAllEnvs correspondant — **TS/JS**
  - REVIEW [FIX] Même problème de portée : `unstubAllGlobals` / `unstubAllEnvs` est accepté partout dans le fichier, sans vérifier `afterEach` / `afterAll`.
- [ ] `testing-no-conditional-assertion` — Signaler if (...) expect(...) dans les tests ; les assertions doivent être inconditionnelles — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `testing-no-try-catch-swallow` — Signaler try { ... } catch { } autour de la phase act dans les tests (masque les erreurs) — **TS/JS**
  - REVIEW [TODO] Reste à reviewer.

## Docker

- [ ] `dockerfile-no-latest-tag` — FROM image:latest (ou :latest implicite) doit utiliser une version tag pinned — **Dockerfile**
- [ ] `dockerfile-pin-exact-version` — Les tags d'image de base doivent pin des versions exactes (ex: node:22.12-alpine3.20) — **Dockerfile**
- [ ] `dockerfile-copy-after-install` — COPY . . ne doit pas apparaître avant l'install des dépendances ; le lockfile doit être copié et installé d'abord — **Dockerfile**
- [ ] `dockerfile-use-npm-ci` — Utiliser npm ci plutôt que npm install dans les Dockerfiles — **Dockerfile**
- [ ] `dockerfile-use-frozen-lockfile` — pnpm/yarn install doit utiliser --frozen-lockfile dans les Dockerfiles — **Dockerfile**
- [ ] `dockerfile-exec-form-cmd` — CMD et ENTRYPOINT doivent utiliser la forme exec ["..."], jamais la forme shell — **Dockerfile**
- [ ] `dockerfile-require-non-root-user` — Le stage de production doit déclarer une directive USER pointant vers un user non-root — **Dockerfile**
- [ ] `dockerfile-require-healthcheck` — Le Dockerfile de production doit contenir une instruction HEALTHCHECK — **Dockerfile**
- [ ] `dockerfile-require-multi-stage` — Le Dockerfile doit utiliser des multi-stage builds (FROM ... AS ...) — **Dockerfile**
- [ ] `dockerfile-no-secrets-in-env` — Les instructions ENV ne doivent pas contenir de valeurs type secret (API keys, tokens) — **Dockerfile**
- [ ] `dockerfile-no-secrets-in-arg` — ARG ne doit pas servir à injecter des secrets ; utiliser --mount=type=secret — **Dockerfile**
- [ ] `dockerfile-no-secrets-in-copy` — COPY ne doit pas inclure .env, *.pem, id_rsa, .npmrc avec tokens — **Dockerfile**
- [ ] `dockerfile-use-cache-mount` — Les étapes RUN de package manager doivent utiliser --mount=type=cache — **Dockerfile**
- [ ] `dockerfile-require-dockerignore` — Un .dockerignore doit exister à côté de chaque Dockerfile — **Dockerfile**
- [ ] `dockerignore-must-exclude-sensitive` — .dockerignore doit lister .git, .env, .env.*, node_modules — **Dockerfile**
- [ ] `compose-no-latest-tag` — Les valeurs image: ne doivent pas utiliser :latest ni omettre le tag — **docker-compose**
- [ ] `compose-no-inline-secrets` — environment: ne doit pas contenir de paires clé/valeur type secret ; utiliser env_file: — **docker-compose**
- [ ] `compose-depends-on-condition` — depends_on doit utiliser la forme longue avec condition: service_healthy quand la dépendance a un healthcheck — **docker-compose**
- [ ] `compose-bind-localhost-ports` — Les ports de services base de données/cache doivent binder sur 127.0.0.1: — **docker-compose**
- [ ] `compose-no-privileged` — Les services ne doivent pas set privileged: true — **docker-compose**
- [ ] `compose-cap-drop-all` — Les services doivent déclarer cap_drop: [ALL] — **docker-compose**
- [ ] `compose-require-resource-limits` — Chaque service doit set deploy.resources.limits.memory — **docker-compose**

## Kubernetes

- [ ] `k8s-no-latest-image-tag` — Interdire image: *:latest ou images sans tag/digest explicite — **YAML**
- [ ] `k8s-require-resource-requests` — Chaque container doit définir resources.requests.cpu et memory — **YAML**
- [ ] `k8s-require-resource-limits` — Chaque container doit définir resources.limits.cpu et memory — **YAML**
- [ ] `k8s-require-liveness-probe` — Chaque container de workload long-running doit définir livenessProbe — **YAML**
- [ ] `k8s-require-readiness-probe` — Chaque container doit définir readinessProbe — **YAML**
- [ ] `k8s-require-run-as-non-root` — securityContext.runAsNonRoot doit être true — **YAML**
- [ ] `k8s-disallow-privilege-escalation` — securityContext.allowPrivilegeEscalation doit être false — **YAML**
- [ ] `k8s-require-read-only-root` — securityContext.readOnlyRootFilesystem doit être true — **YAML**
- [ ] `k8s-require-drop-all-caps` — securityContext.capabilities.drop doit inclure ALL — **YAML**
- [ ] `k8s-min-replicas-two` — Deployments doivent avoir replicas >= 2 (ou HPA minReplicas >= 2) — **YAML**
- [ ] `k8s-rolling-update-zero-unavailable` — strategy.rollingUpdate.maxUnavailable doit être 0 — **YAML**
- [ ] `k8s-require-pod-disruption-budget` — Chaque Deployment/StatefulSet production doit avoir un PodDisruptionBudget — **YAML**
- [ ] `k8s-no-secrets-in-configmap` — Les données ConfigMap ne doivent pas contenir de clés type PASSWORD, TOKEN, KEY, SECRET — **YAML**
- [ ] `k8s-no-plaintext-secret-in-git` — Interdire kind: Secret avec data/stringData populated commité au repo — **YAML**
- [ ] `k8s-require-explicit-namespace` — Les manifests doivent set metadata.namespace (pas de default implicite) — **YAML**
- [ ] `k8s-require-standard-labels` — Les resources doivent inclure app.kubernetes.io/name et app.kubernetes.io/instance — **YAML**
- [ ] `k8s-no-default-service-account` — Les pods doivent set serviceAccountName (pas utiliser default) — **YAML**
- [ ] `k8s-rbac-no-wildcard-verbs` — Les rules Role/ClusterRole ne doivent pas utiliser verbs: ["*"] — **YAML**
- [ ] `k8s-rbac-no-wildcard-resources` — Les rules Role/ClusterRole ne doivent pas utiliser resources: ["*"] — **YAML**
- [ ] `k8s-require-network-policy` — Chaque namespace avec workloads doit avoir une NetworkPolicy default-deny — **YAML**
- [ ] `k8s-require-ingress-tls` — Les resources Ingress doivent définir spec.tls — **YAML**

## Shadcn

- [ ] `shadcn-no-raw-tailwind-colors` — Interdire les couleurs raw Tailwind (bg-blue-500, text-gray-600) en JSX ; exiger les tokens sémantiques (bg-primary, text-muted-foreground) — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-no-space-x-y` — Interdire space-x-*/space-y-* ; utiliser flex + gap-* — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-no-manual-dark-overrides` — Interdire dark:bg-*/dark:text-* paired avec des couleurs light explicites ; exiger les tokens sémantiques — **TSX**
  - REVIEW [FIX] La notion `paired avec des couleurs light explicites` n'est pas vérifiée. Un `dark:bg-gray-900` isolé est signalé même sans couleur light correspondante.
- [ ] `shadcn-no-manual-zindex-overlays` — Interdire z-* className sur Dialog, Sheet, Drawer, AlertDialog, DropdownMenu, Popover, Tooltip — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-dialog-requires-title` — Exiger <DialogTitle> dans chaque <DialogContent> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-sheet-requires-title` — Exiger <SheetTitle> dans chaque <SheetContent> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-avatar-requires-fallback` — Exiger <AvatarFallback> dans chaque <Avatar> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-tabs-trigger-in-list` — <TabsTrigger> doit être descendant de <TabsList>, pas directement dans <Tabs> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-no-hr-use-separator` — Interdire <hr> brut en JSX ; exiger <Separator /> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-no-custom-skeleton` — Interdire <div className="animate-pulse ..."> ; exiger <Skeleton> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-no-custom-badge` — Interdire <span className="rounded-full bg-*"> badge-like ; exiger <Badge> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `shadcn-button-icon-data-attr` — Exiger data-icon="inline-start"|"inline-end" sur les icônes dans <Button> ; interdire mr-2/ml-2 — **TSX**
  - REVIEW [FIX] L'exigence `data-icon` n'est pas implémentée. Un `<Button><Icon />Save</Button>` passe tant que l'icône n'a pas `mr-2` / `ml-2`.
- [ ] `shadcn-no-toggle-group-manual` — Signaler .map() rendant <Button> avec variant conditionnel ; exiger <ToggleGroup> + <ToggleGroupItem> — **TSX**
  - REVIEW [TODO] Reste à reviewer.

## React Native

- [ ] `rn-no-react-navigation-stack` — Interdire createStackNavigator/@react-navigation/stack ; utiliser Expo Router — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-no-string-route-names` — Interdire navigation.navigate('RouteName', params) ; utiliser router.push('/path') typé — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-auth-token-securestore` — Interdire AsyncStorage pour auth_token/authToken ; exiger expo-secure-store — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-flashlist-over-flatlist` — Signaler FlatList de react-native ; exiger FlashList de @shopify/flash-list — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-no-inline-renderitem` — Interdire les arrow functions inline en renderItem sur FlatList/FlashList ; exiger un composant extrait — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-memo-list-items` — Les composants list item utilisés en renderItem doivent être wrappés dans React.memo — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-no-inline-styles` — Interdire les objets style inline (style={{ ... }}) ; exiger StyleSheet.create ou useMemo — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-reanimated-over-animated` — Interdire Animated.timing/Animated.Value de react-native ; exiger react-native-reanimated — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-router-replace-after-login` — Interdire router.push après login/logout ; exiger router.replace() — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-push-permissions-before-token` — Interdire getExpoPushTokenAsync sans requestPermissionsAsync préalable — **TSX**
  - REVIEW [FIX] L'ordre "avant" n'est pas vérifié : un `requestPermissionsAsync()` placé après `getExpoPushTokenAsync()` dans la même fonction suffit à faire passer la règle.
- [ ] `rn-push-token-requires-projectid` — getExpoPushTokenAsync doit être appelé avec { projectId } — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-raw-string-in-text` — Interdire les string/number comme JSX children en dehors de <Text> dans les composants RN — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-image-source-object` — Interdire <Image source="url"> avec string literal ; exiger { uri: string } ou require() — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-flashlist-estimated-item-size` — Exiger estimatedItemSize prop sur <FlashList> — **TSX**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `rn-biometrics-hardware-check` — Exiger hasHardwareAsync/isEnrolledAsync avant authenticateAsync — **TSX**
  - REVIEW [FIX] Comme pour les permissions push, l'implémentation vérifie seulement la présence des appels dans la même fonction, pas qu'ils précèdent `authenticateAsync()` ni que leurs résultats soient utilisés pour gate l'authentification.
- [ ] `rn-expo-router-layout-required` — Exiger _layout.tsx dans chaque group directory Expo Router — **TSX**
  - REVIEW [TODO] Reste à reviewer.

## CI/CD

- [ ] `ci-checkout-action-pinned` — actions/checkout doit être pinned à @v4+ — **GitHub Actions YAML**
- [ ] `ci-setup-node-cache-enabled` — actions/setup-node doit inclure cache: 'npm' ou équivalent — **GitHub Actions YAML**
- [ ] `ci-use-npm-ci` — Les étapes CI d'install doivent utiliser npm ci, pas npm install — **GitHub Actions YAML**
- [ ] `ci-no-hardcoded-db-password` — POSTGRES_PASSWORD doit référencer ${{ secrets.* }}, pas un literal — **GitHub Actions YAML**
- [ ] `ci-postgres-healthcheck` — Le bloc services Postgres doit inclure --health-cmd pg_isready — **GitHub Actions YAML**
- [ ] `ci-playwright-report-upload` — Le job E2E doit upload playwright-report/ avec if: failure() — **GitHub Actions YAML**
- [ ] `ci-cache-key-includes-lockfile` — Les clés actions/cache doivent inclure hashFiles('**/package-lock.json') — **GitHub Actions YAML**
- [ ] `ci-docker-gha-cache` — docker/build-push-action doit set cache-from/to: type=gha — **GitHub Actions YAML**
- [ ] `ci-no-plaintext-secrets` — Aucune valeur env/with de workflow ne doit contenir de literal password/token/key ; utiliser ${{ secrets.* }} — **GitHub Actions YAML**

## UI / UX / Animations

- [ ] `ui-no-transition-all` — Interdire transition: all et transition-property: all ; lister les propriétés explicitement — **CSS/TSX**
- [ ] `ui-animate-transform-opacity-only` — Les animations ne doivent cibler que transform et opacity ; interdire top, left, width, height, margin, padding — **CSS/TSX**
- [ ] `ui-tabular-nums-on-data` — Les éléments affichant des données numériques (counters, prix, metrics) doivent utiliser tabular-nums — **CSS/TSX**
  - REVIEW [FIX] La règle est annoncée **CSS/TSX**, mais l'implémentation actuelle ne couvre que TS/TSX. Les sélecteurs CSS numériques sans `font-variant-numeric: tabular-nums` ne seront pas contrôlés.
- [ ] `ui-text-balance-headings` — h1-h6 doivent set text-wrap: balance — **CSS/TSX**
- [ ] `ui-antialiased-on-root` — Le root/html doit avoir -webkit-font-smoothing: antialiased — **CSS/TSX**
- [ ] `ui-no-pure-black` — Interdire #000, #000000, rgb(0,0,0), black dans les styles — **CSS/TSX**
- [ ] `ui-concentric-border-radius` — Quand un enfant est dans un parent avec padding, le border-radius enfant = parent - padding — **CSS/TSX**
- [ ] `ui-prefers-reduced-motion` — Les CSS avec animation/transition doivent inclure @media (prefers-reduced-motion: reduce) — **CSS**
- [ ] `ui-hover-gated-media-query` — Les :hover avec transform/scale doivent être dans @media (hover: hover) and (pointer: fine) — **CSS**
- [ ] `ui-min-hit-area-44` — Les éléments interactifs doivent avoir une zone de tap ≥ 44x44px — **CSS/TSX**
  - REVIEW [FIX] La règle est annoncée **CSS/TSX**, mais l'implémentation actuelle ne couvre que TS/TSX. Les petites zones de clic définies en CSS ne seront pas contrôlées.
  - REVIEW [FIX] L'implémentation TSX ne détecte que les couples `h-*`/`w-*` ou `size-*` minuscules ; elle ne couvre pas l'exemple de la spec `px-2 py-1 text-xs`, qui peut produire un tap target trop petit sans dimensions explicites.
- [ ] `ui-no-display-none-exit` — Interdire display: none comme seul traitement de sortie sur les éléments animés ; exiger opacity + translate — **CSS/TSX**
- [ ] `ui-exit-duration-shorter-enter` — La durée d'animation de sortie doit être ≤ la durée d'entrée — **TSX (motion)**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ui-animate-presence-requires-exit` — <motion.*> dans <AnimatePresence> doit définir un prop exit — **TSX (motion)**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ui-no-scroll-trigger-markers-prod` — ScrollTrigger markers: true doit être gardé par process.env.NODE_ENV — **TSX (gsap)**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ui-stagger-children-cap` — staggerChildren doit être ≤ 0.05 (50ms) — **TSX (motion)**
  - REVIEW [TODO] Reste à reviewer.
- [ ] `ui-no-keyframes-for-interruptible` — Interdire @keyframes pour les animations state-driven (class-toggled) ; exiger transition — **CSS**
- [ ] `ui-symmetric-initial-exit` — Les props initial et exit de motion doivent partager les mêmes clés (forme miroir) — **TSX (motion)**
  - REVIEW [TODO] Reste à reviewer.
