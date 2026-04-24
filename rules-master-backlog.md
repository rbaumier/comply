# Master Backlog — Règles à implémenter

Dernière mise à jour: 2026-04-24

## Résumé

**988 règles implémentées.** Le backlog est quasi-vide.

| Catégorie | Implémenté | Restant |
|-----------|------------|---------|
| Haute priorité (perf/infra) | 9/9 | 0 |
| Modernization ES2022+ | 12/12 | 0 |
| Security | 3/3 | 0 |
| Architecture | 10/10 | 0 |
| Code Quality | 12/12 | 0 |
| Rust-specific | 3/3 | 0 |
| Tier 3 (type info / tsc) | 7/8 | 1 |
| React (oxc_semantic) | 2/2 | 0 |
| Tier 5/6 (LLM review) | — | supprimé |
| Bloquées infra | 0/121 | 121 |

**Total actionnable restant: 1 règle** (`data-clumps` cross-file, bloquée infra)

---

## Règles restantes

### Tier 3 — Type info rules

| Règle | Description | Status |
|-------|-------------|--------|
| `strict-typing` | Détection `any` inféré | ✓ Couvert par `no-unsafe-*` famille tsgolint |
| `misleading-name` | Nom vs type réel | ✓ Couvert par `no-misleading-collection-name` (oxc_semantic) |
| `boundary-condition` | Validation aux frontières | ✓ Implémenté (tree-sitter, 2026-04-24) |
| `data-clumps` (via tsc) | Version cross-file avec types | Bloqué infra (cross-file type resolution) |

**Déjà implémentées (heuristique sans tsc):**
- `option-vs-result` ✓
- `no-raw-db-entity-in-handler` ✓
- `structured-api-error` ✓
- `api-first` ✓

### React — oxc_semantic (2026-04-24)

| Règle | Description | Status |
|-------|-------------|--------|
| `no-redundant-state` | `useState` dont le setter n'est jamais utilisé | ✓ |
| `unused-component-prop` | Prop déclarée mais jamais lue dans le composant | ✓ |

### Tier 5/6 — LLM Review (supprimé)

Subsystem LLM supprimé (2026-04-24). Les 26 règles LLM ne sont plus actionnables.

### Bloquées sur infrastructure (121 règles)

Détail dans `docs/plugin-rules-todo.md`. Non actionnables sans infra préalable.

| Infra requise | Count |
|---------------|-------|
| Type checker (tsc) | 55 |
| Module resolution | 28 |
| Scope analysis | 19 |
| Full regex parser | 7 |
| JSDoc type context | 5 |
| Unicorn infra | 7 |

---

## Règles complétées (pour référence)

### Haute priorité — DONE

- ✓ `no-clones` (clone-detection native)
- ✓ `prefer-nullish-coalescing`
- ✓ `ban-dependencies`
- ✓ `prefer-static-regex`
- ✓ `arguments-order` (cross-file)
- ✓ `no-horizontal-folders`
- ✓ `no-barrel-re-export-all`
- ✓ ImportIndex tsconfig paths
- ✓ ImportIndex Vue SFC

### Modernization ES2022+ — DONE

- ✓ `prefer-array-to-reversed`
- ✓ `prefer-array-to-sorted`
- ✓ `prefer-array-to-spliced`
- ✓ `prefer-array-fill`
- ✓ `prefer-array-from-map`
- ✓ `prefer-object-has-own`
- ✓ `prefer-exponentiation-operator`
- ✓ `prefer-url-canparse`
- ✓ `prefer-timer-args`
- ✓ `no-indexof-equality`

### Security — DONE

- ✓ `os-command`
- ✓ `post-message`
- ✓ `xpath`

### Architecture — DONE

- ✓ `vertical-slice-no-role-folders`
- ✓ `feature-boundary-strict`
- ✓ `no-global-types-file`
- ✓ `colocate-hook-with-component`
- ✓ `single-call-site-inline`
- ✓ `data-clumps` (single-file)
- ✓ `symmetric-pairs` (single-file)

### Code Quality — DONE

- ✓ `no-let-var`
- ✓ `no-imperative-loops`
- ✓ `no-mutation-methods`
- ✓ `no-this-mutation`
- ✓ `no-property-mutation`
- ✓ `require-exhaustive-switch`
- ✓ `function-inside-loop`
- ✓ `function-return-type`
- ✓ `no-selector-parameter`
- ✓ `no-long-closure`
- ✓ `callable-burden-hotspot`
- ✓ `no-top-heavy-file`

### Rust-specific — DONE

- ✓ `rust-prefer-strum`
- ✓ `rust-deep-nesting-early-return`
- ✓ `rust-explicit-iter-loop`

---

## Notes

- Les 118 règles de `RULES_TO_ADD.md` sont toutes implémentées.
- `docs/unicorn-rules-catalog.md`: 139/147 implémentées (7 bloquées infra).
- `docs/sonar-candidates.md`: 100/106 SonarJS implémentées.
- Rationale des règles: dans le code (docblock) + commit messages, jamais dans ce backlog.

---

## Eslint plugins à explorer

Voir fin du fichier original pour la liste complète des plugins à explorer.
