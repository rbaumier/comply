# Master Backlog — Règles à implémenter

Dernière mise à jour: 2026-04-22

Compilation exhaustive des règles non encore implémentées dans comply, extraite de tous les `.md` du repo (hors `DIFF_REVIEW_FULL.md`, raw output).

**Sources analysées:**
- `RULES_TO_ADD.md` — 118/118 ✅ (zéro restant)
- `RULES_TODO.md` — 2 entrées TODO actives
- `RULES_TO_FIX.md` — 1 entrée ⏸ (paused)
- `TODO.md` — Tier 0/3/5/6 + unicorn blocked
- `TODO_AFTER_REVIEWS.md` — classification review POC (bugs dans règles existantes, pas de nouvelles règles)
- `docs/rules-backlog.md` — SonarJS, Hickey, TokenMap, e18e, Vertical Codebase
- `docs/hickey-rules-todo.md` — Rich Hickey "Simple Made Easy"
- `docs/plugin-rules-todo.md` — 121 rules bloquées sur infra
- `docs/rule-scope-expansion.md` — expansion backends existants
- `docs/sonar-candidates.md`, `docs/sonar-remaining.md` — SonarJS restantes
- `docs/project-scope-multi-lang.md` — design multi-langage
- `docs/unicorn-rules-catalog.md` — catalogue unicorn (147/147 implémentables déjà faites)

**Total règles à implémenter**: ~186 (hors 121 bloquées sur infrastructure manquante)

---

## Par priorité

### Haute priorité (infra/perf bloquante, ou règles largement demandées)

| Règle | Description | Langages | Source |
|-------|-------------|----------|--------|
| `no-clones` (native) | Clone-detection in-process remplaçant jscpd (Tier 0 perf debt — 92% du wall-clock actuel) | TS/JS/TSX + Rust | TODO.md §Tier 0 |
| `prefer-nullish-coalescing` | `x != null ? x : y` → `x ?? y` | TS/JS/TSX | rules-backlog.md §e18e |
| `ban-dependencies` | Bannir lodash, moment, underscore | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-static-regex` | Hisser regex hors fonctions (étendre `react_hoist_regex`) | TS/JS/TSX + Rust | rules-backlog.md §e18e |
| `arguments-order` cross-file | Comparer définition vs call-sites cross-file | TS/JS/TSX | rules-backlog.md |
| ImportIndex tsconfig paths | Résolution des alias `@/...` | TS/JS/TSX | rules-backlog.md |
| ImportIndex Vue SFC | `<script setup>`, defineExpose/Props/Emits | Vue | rules-backlog.md + project-scope-multi-lang.md §5 |
| `no-horizontal-folders` | Bannir imports depuis `src/utils/*`, `src/hooks/*`, `src/types/*` | TS/JS/TSX | rules-backlog.md §Vertical Codebase |
| `no-barrel-re-export-all` | Interdire `export * from` dans `index.ts` | TS/JS/TSX | rules-backlog.md §Vertical Codebase |

### Moyenne priorité

| Règle | Description | Langages | Source |
|-------|-------------|----------|--------|
| `vertical-slice-no-role-folders` | Flag `src/services/` + `src/repositories/` + `src/handlers/` en parallèle | TS/JS/TSX + Rust | RULES_TODO.md §3 |
| `os-command` | exec/spawn avec shell injection | TS/JS/TSX + Rust | rules-backlog.md §SonarJS |
| `post-message` | `postMessage` sans target origin | TS/JS/TSX | rules-backlog.md §SonarJS |
| `xpath` | Injection XPath | TS/JS/TSX + Rust | rules-backlog.md §SonarJS |
| `function-inside-loop` | Fonction définie dans une boucle | TS/JS/TSX + Rust | rules-backlog.md §SonarJS (Note: était implémentée, supprimée, à reconsidérer) |
| `function-return-type` | Retourne des types incohérents | TS/JS/TSX | rules-backlog.md §SonarJS |
| `no-selector-parameter` | Booléen "sélecteur" → séparer en 2 fonctions | TS/JS/TSX + Rust | rules-backlog.md §SonarJS |
| `no-let-var` | Bannir `let` (améliorer `prefer-const`) | TS/JS/TSX | hickey-rules-todo.md §1 + rules-backlog.md §Hickey |
| `no-imperative-loops` | Ajouter while/do-while au ban | TS/JS/TSX | hickey-rules-todo.md + rules-backlog.md §Hickey |
| `no-mutation-methods` | Étendre ban des méthodes mutantes | TS/JS/TSX | rules-backlog.md §Hickey |
| `no-this-mutation` | Mutation de `this` hors constructeur | TS/JS/TSX | rules-backlog.md §Hickey |
| `no-property-mutation` | Interdire `obj.prop = value` | TS/JS/TSX | hickey-rules-todo.md §1.4 |
| `require-exhaustive-switch` | Switch sur unions discriminées (weak heuristic sans tsc) | TS/JS/TSX | hickey-rules-todo.md §4.1 |
| `feature-boundary-strict` | Feature A ne peut importer Feature B que via `index.ts` | TS/JS/TSX + Vue | rules-backlog.md §Vertical Codebase |
| `no-global-types-file` | Interdire `types.ts` à la racine | TS/JS/TSX | rules-backlog.md §Vertical Codebase |
| `prefer-array-to-reversed` | `[...arr].reverse()` → `arr.toReversed()` (ES2023) | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-array-to-sorted` | `arr.slice().sort()` → `arr.toSorted()` (ES2023) | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-array-to-spliced` | Idem pour splice (ES2023) | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-array-fill` | `Array.from({length: n}, () => v)` → `Array(n).fill(v)` | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-object-has-own` | `obj.hasOwnProperty(k)` → `Object.hasOwn(obj, k)` (ES2022) | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-exponentiation-operator` | `Math.pow(x, y)` → `x ** y` | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-url-canparse` | try-catch `new URL()` → `URL.canParse()` | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-timer-args` | `setTimeout(() => fn(a), 100)` → `setTimeout(fn, 100, a)` | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-array-from-map` | `[...iter].map(fn)` → `Array.from(iter, fn)` | TS/JS/TSX | rules-backlog.md §e18e |
| `no-indexof-equality` | `str.indexOf('x') === 0` → `str.startsWith('x')` | TS/JS/TSX | rules-backlog.md §e18e |
| `data-clumps` cross-file | Étendre au cross-file | TS/JS/TSX + Rust | rules-backlog.md |
| `symmetric-pairs` cross-file | Étendre au cross-file (par feature) | TS/JS/TSX + Rust | rules-backlog.md |
| `no-long-closure` | Arrow multi-lignes (seuil strict) | TS/JS/TSX | rules-backlog.md §TokenMap |
| `rust-prefer-strum` | Enum avec `impl Display + FromStr` manuels → `strum` | Rust | RULES_TODO.md §9 |
| `rust-deep-nesting-early-return` | Nesting ≥4 avec `if let Some` guards → `let-else` | Rust | RULES_TODO.md §9 |

### Basse priorité

| Règle | Description | Langages | Source |
|-------|-------------|----------|--------|
| `single-call-site-inline` | Fonction exportée référencée dans 1 seul fichier | TS/JS/TSX + Rust | RULES_TODO.md §3 |
| `rust-explicit-iter-loop` | Fix FP: `for &b in bytes.iter()` (paused) | Rust | RULES_TO_FIX.md §7 |
| `callable-burden-hotspot` | Score composite (lignes × complexité) | TS/JS/TSX + Rust | rules-backlog.md §TokenMap |
| `no-top-heavy-file` | Top 3 callables > X% du burden | TS/JS/TSX + Rust | rules-backlog.md §TokenMap |
| `colocate-hook-with-component` | Hook spécifique → même fichier/dossier | TSX | rules-backlog.md §Vertical Codebase |
| `no-identical-functions` alpha-rename | Normalize variables locales | TS/JS/TSX + Rust | rules-backlog.md |
| `inconsistent-function-call` namespace | Support `ns.Foo()` | TS/JS/TSX | rules-backlog.md |
| `prefer-array-some` extension | Ajouter `arr.find(pred) !== undefined` | TS/JS/TSX | rules-backlog.md §e18e |
| `prefer-spread` extension | Ajouter `Object.assign({}, a, b)` et `fn.apply()` | TS/JS/TSX | rules-backlog.md §e18e |
| ImportIndex SQL cross-migrations | Dépendances cross-migrations | SQL | rules-backlog.md + project-scope-multi-lang.md §6 |

---

## Par catégorie

### Modernization (ES2022+)

Toutes TS/JS/TSX — source `docs/rules-backlog.md §e18e`.

- `prefer-nullish-coalescing`
- `prefer-array-to-reversed`, `prefer-array-to-sorted`, `prefer-array-to-spliced`
- `prefer-array-fill`
- `prefer-object-has-own`
- `prefer-exponentiation-operator`
- `prefer-url-canparse`
- `prefer-timer-args`
- `prefer-array-from-map`
- `no-indexof-equality`
- `prefer-static-regex`
- `ban-dependencies`

### Security

- `os-command` (TS/JS/TSX + Rust)
- `post-message` (TS/JS/TSX)
- `xpath` (TS/JS/TSX + Rust)

### Architecture

- `vertical-slice-no-role-folders` (multi)
- `no-horizontal-folders` (TS/JS/TSX)
- `no-global-types-file` (TS/JS/TSX)
- `feature-boundary-strict` (TS/JS/TSX + Vue)
- `no-barrel-re-export-all` (TS/JS/TSX)
- `colocate-hook-with-component` (TSX)
- `single-call-site-inline` (multi)
- `data-clumps` cross-file (multi)
- `symmetric-pairs` cross-file (multi)
- `arguments-order` cross-file (multi)

### Performance

- `no-clones` native (multi) — remplace jscpd, Tier 0
- `prefer-static-regex` (TS/JS/TSX + Rust)

### Code Quality

- `no-let-var` (TS/JS/TSX)
- `no-imperative-loops` (TS/JS/TSX)
- `no-mutation-methods` (TS/JS/TSX)
- `no-this-mutation` (TS/JS/TSX)
- `no-property-mutation` (TS/JS/TSX)
- `require-exhaustive-switch` (TS/JS/TSX)
- `function-return-type` (TS/JS/TSX)
- `no-selector-parameter` (TS/JS/TSX + Rust)
- `function-inside-loop` (TS/JS/TSX + Rust) — à reconsidérer après suppression
- `no-long-closure` (TS/JS/TSX)
- `callable-burden-hotspot` (multi)
- `no-top-heavy-file` (multi)

### Rust-specific

- `rust-explicit-iter-loop` (fix paused)
- `rust-prefer-strum`
- `rust-deep-nesting-early-return`

### Infrastructure / cross-file

- ImportIndex tsconfig paths (TS/JS/TSX)
- ImportIndex Vue SFC (Vue) — priorité MEDIUM dans project-scope-multi-lang.md
- ImportIndex SQL cross-migrations — priorité LOW
- `ProjectCheck` trait (backend cross-file)
- Index call-sites namespace (tracker `ns.Symbol()`)
- Transitive re-exports (flattening `export * from`)

### LLM / Review-only — Tier 5 (8 règles)

Source: `TODO.md §Tier 5`. Philosophy/architecture rules nécessitant review LLM.

- `parse-dont-validate`
- `make-invalid-states-unrepresentable`
- `functional-core-imperative-shell`
- `document-impossible-states`
- `bound-every-input` (rejection at boundary)
- `crosscutting-via-wrapping` (ex: `withTracing`)
- `map-db-entities-to-dtos`
- `error-messages-as-step-by-step-remediation`
- `command_injection_review` (bonus, mémoire `project_comply_next_steps.md`)
- `path_traversal_review` (bonus, idem)

### LLM / Architectural — Tier 6 (18 règles)

Source: `TODO.md §Tier 6`. Règles cross-project / architecturales via LLM.

- `reuse-before-creating`
- `rule-of-three`
- `prefer-boring-technology`
- `dry-repo-wide`
- `vertical-slices`
- `shotgun-surgery`
- `divergent-change`
- `information-leakage`
- `srp-per-function-module`
- `cqs-command-or-query`
- `composition-over-inheritance`
- `tests-linting-ci-cd-from-day-1`
- `constrain-first-relax-later`
- `codebase-homogeneity`
- `structural-guardrails-over-discipline`
- `hard-cutover-on-migrations`
- `pin-all-versions`
- `group-tests-by-feature-not-type`

### Tier 3 — Needs type info (tsc pipeline) — 8 règles

Source: `TODO.md §Tier 3`. Requiert `comply typecheck` subcommand.

- `strict-typing` (inferred `any`)
- `option-vs-result`
- `misleading-name`
- `data-clumps` (via tsc)
- `boundary-condition`
- `no-raw-db-entity-in-handler`
- `structured-api-error`
- `api-first`

### BLOQUÉES sur infrastructure (121 règles) — hors scope immédiat

Source: `docs/plugin-rules-todo.md`. Listées pour référence, non actionnables sans infra préalable.

| Infra requise | Plugin(s) | Count |
|---------------|-----------|-------|
| Type checker (tsc) | typescript-eslint | 55 |
| Module resolution | eslint-plugin-import + eslint-plugin-n | 28 |
| Scope analysis | eslint-plugin-react + eslint-plugin-playwright | 19 |
| Full regex parser | eslint-plugin-regexp | 7 |
| JSDoc type context | eslint-plugin-jsdoc | 5 |
| Unicorn infra (browserslist, scope) | eslint-plugin-unicorn | 7 |

Détail complet dans `docs/plugin-rules-todo.md`.

---

## Par langage

### Multi-langage (TS/JS + Rust)

- `no-clones` native
- `os-command`
- `xpath`
- `vertical-slice-no-role-folders`
- `single-call-site-inline`
- `data-clumps` cross-file
- `symmetric-pairs` cross-file
- `arguments-order` cross-file
- `function-inside-loop`
- `no-selector-parameter`
- `prefer-static-regex`
- `callable-burden-hotspot`
- `no-top-heavy-file`

### TypeScript / JavaScript / TSX uniquement

**Modernization ES:**
- `prefer-nullish-coalescing`, `prefer-array-to-reversed`, `prefer-array-to-sorted`, `prefer-array-to-spliced`, `prefer-array-fill`, `prefer-object-has-own`, `prefer-exponentiation-operator`, `prefer-url-canparse`, `prefer-timer-args`, `prefer-array-from-map`, `no-indexof-equality`, `ban-dependencies`

**Immutability / Hickey:**
- `no-let-var`, `no-imperative-loops`, `no-mutation-methods`, `no-this-mutation`, `no-property-mutation`, `require-exhaustive-switch`

**Architecture:**
- `no-horizontal-folders`, `no-global-types-file`, `no-barrel-re-export-all`, `function-return-type`, `no-long-closure`

**Security (JS-specific):**
- `post-message`

### TSX uniquement

- `colocate-hook-with-component`

### Vue uniquement

- `feature-boundary-strict` (TS/JS/TSX + Vue)
- ImportIndex Vue SFC

### Rust uniquement

- `rust-explicit-iter-loop` (fix)
- `rust-prefer-strum`
- `rust-deep-nesting-early-return`

### SQL uniquement

- ImportIndex SQL cross-migrations

### Universal (tous langages)

Aucune nouvelle règle universelle en attente. Les règles text-based (secrets, IP, http://, commentaires) ont été étendues à Rust dans `docs/rule-scope-expansion.md`.

---

## Notes

- **Les 118 règles de `RULES_TO_ADD.md` sont toutes implémentées** (zod, tanstack, vue, tailwind, react, i18n, security, ts/arch, drizzle, sql, api, rust, testing, better-auth).
- **Les fichiers `RULES_TO_FIX.md` et `TODO_AFTER_REVIEWS.md`** concernent des FP sur règles existantes (37/37 résolus pour RULES_TO_FIX, 1 paused), pas de nouvelles règles à créer.
- **`docs/unicorn-rules-catalog.md`** : 139/147 implémentées, les 7 restantes sont bloquées infra (catégorie ci-dessus).
- **`docs/sonar-candidates.md` + `docs/sonar-remaining.md`** : 100/106 SonarJS implémentées; les 6 restantes sont listées ici. 159 autres sont hors-scope (AWS, browser runtime, regex internals, style, framework-specific).
- **`docs/rule-scope-expansion.md`** : expansion backends existants (ajouter Rust à des TextCheck/AstCheck existants) — travail en partie fait; vérifier par règle avant de commencer.
- Rationale des règles : jamais dans ce backlog, toujours dans le code (docblock) + commit messages (git log). Cf. mémoire `feedback_rule_rationale_in_code.md`.

---

### Eslint plugins à explorer 

###### Exploration 1

https://github.com/francoismassart/eslint-plugin-tailwindcss
https://github.com/schoero/eslint-plugin-better-tailwindcss
https://github.com/vitest-dev/eslint-plugin-vitest
https://github.com/eslint-stylistic/eslint-stylistic
https://github.com/un-ts/eslint-plugin-import-x
https://github.com/typescript-eslint/typescript-eslint
https://github.com/jfmengels/eslint-plugin-fp
https://github.com/mskelton/eslint-plugin-playwright
https://github.com/eslint-functional/eslint-plugin-functional
https://github.com/javierbrea/eslint-plugin-boundaries
https://github.com/mozilla/eslint-plugin-no-unsanitized
https://github.com/dukeluo/eslint-plugin-check-file
https://github.com/edvardchen/eslint-plugin-i18next
https://github.com/ArnaudBarre/eslint-plugin-react-refresh
https://github.com/antfu/eslint-plugin-antfu
https://github.com/cartant/eslint-plugin-etc
https://github.com/CodelyTV/eslint-plugin-hexagonal-architecture
https://github.com/microsoft/eslint-plugin-sdl
https://github.com/mysticatea/eslint-plugin-es
https://github.com/godaddy/eslint-plugin-i18n-json
https://github.com/getify/eslint-plugin-proper-arrows
https://github.com/es-tooling/eslint-plugin-depend
https://github.com/nickjvandyke/eslint-plugin-react-you-might-not-need-an-effect
https://github.com/cvazac/eslint-plugin-react-perf
https://github.com/Igorkowalski94/eslint-plugin-project-structure
https://github.com/Shopify/web-configs/blob/main/packages/eslint-plugin/README.md
https://github.com/thepassle/eslint-plugin-barrel-files
https://github.com/eslint-community/eslint-plugin-es-x
https://github.com/lukastaegert/eslint-plugin-tree-shaking
https://github.com/gkouziik/eslint-plugin-security-node
https://github.com/BohdanTkachenko/eslint-plugin-require-path-exists
https://github.com/marcalexiei/eslint-plugin-zod
https://github.com/nickdeis/eslint-plugin-no-secrets
https://github.com/effozen/eslint-plugin-fsd-lint
https://github.com/yeonjuan/html-eslint
https://github.com/rlaffers/eslint-plugin-xstate
https://github.com/stackblitz/eslint-plugin
https://github.com/ota-meshi/eslint-plugin-toml
https://github.com/chejen/eslint-plugin-i18n
https://github.com/azat-io/eslint-plugin-de-morgan
https://github.com/ota-meshi/eslint-plugin-markdown-preferences
https://github.com/andykao1213/eslint-plugin-react-hook-form
https://github.com/Akronae/eslint-plugin-exception-handling
https://github.com/rich-lab/eslint-plugin-financial
https://github.com/EvgenyOrekhov/eslint-config-hardcore
https://github.com/SebastienGllmt/eslint-plugin-no-floating-promise
https://github.com/foad/eslint-plugin-listeners
https://github.com/temoncher/eslint-plugin-clsx
https://github.com/feature-sliced/eslint-config
https://github.com/snyk-labs/eslint-plugin-react-security
https://github.com/kantord/eslint-plugin-write-good-comments
https://github.com/idmitriev/eslint-plugin-better
https://github.com/shiva-hack/eslint-plugin-pii

###### Exploration 2

Shopify/web-configs/blob/main/packages/eslint-plugin/README.md 
https://github.com/upleveled/eslint-plugin-upleveled
https://github.com/PeterKwesiAnsah/eslint-plugin-goodeffects
https://github.com/jeremy-deutsch/eslint-plugin-jsx-falsy
https://github.com/baseballyama/eslint-plugin-postgresql
https://github.com/Angelelz/eslint-plugin-drizzle
https://github.com/ruidosujeira/perf-linter
https://github.com/RexSkz/eslint-plugin-try-catch-failsafe
https://github.com/tjenkinson/eslint-plugin-redos-detector
https://github.com/typed-rocks/eslint-plugin-typed-rocks
https://github.com/JonnyBurger/eslint-plugin-small-import
https://github.com/gkiely/eslint-plugin-jsx-no-leaked-values
https://github.com/vitalets/eslint-plugin-visual-complexity
https://github.com/YashJadhav21/eslint-plugin-ai-guard
https://github.com/zeronone/eslint-plugin-const-immutable
https://github.com/aryelu/eslint-plugin-code-complete
https://github.com/samchungy/eslint-plugin-import-zod
https://github.com/nene/eslint-plugin-no-null
https://github.com/nebrius/eslint-plugin-fast-import
https://github.com/AndreaPontrandolfo/eslint-plugin-fsecond
https://github.com/julianburr/eslint-plugin-jsx-conditionals
https://github.com/gajus/eslint-plugin-jsdoc#user-content-eslint-plugin-jsdoc-rules
https://github.com/susisu/eslint-plugin-safe-typescript
https://ota-meshi.github.io/eslint-plugin-math/
https://github.com/tigerabrodi/eslint-plugin-react-query-keys
https://github.com/regru/eslint-plugin-prefer-early-return
https://github.com/mizdra/eslint-plugin-layout-shift
https://github.com/infofarmer/eslint-plugin-jsx-a11y
https://github.com/betaorbust/eslint-plugin-pocket-fluff/tree/main
https://github.com/es-joy/eslint-plugin-radar
https://github.com/shuckster/eslint-plugin-big-number-rules
https://github.com/artlaman/eslint-plugin-index
https://github.com/ota-meshi/eslint-plugin-node-dependencies
https://github.com/shian15810/eslint-plugin-typescript-enum
https://github.com/artalar/eslint-plugin-react-component-name
