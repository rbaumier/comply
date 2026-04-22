# Rules Backlog

Règles à ajouter ou améliorer. Dernière mise à jour: 2026-04-22

---

## Nouvelles règles à implémenter

### SonarJS restantes (6)

| Règle | Description | Faisabilité |
|-------|-------------|-------------|
| `os-command` | Détecte exec/spawn avec shell injection | moyen |
| `post-message` | postMessage sans target origin | moyen |
| `xpath` | Injection XPath | moyen |
| `function-inside-loop` | Fonction définie dans une boucle | facile |
| `function-return-type` | Retourne des types incohérents | moyen |
| `no-selector-parameter` | Booléen "sélecteur" → séparer en 2 fonctions | moyen |

### Rich Hickey "Simple Made Easy" (2 TODO + 4 partielles)

| Règle | Description | Faisabilité |
|-------|-------------|-------------|
| `no-property-mutation` | Interdire `obj.prop = value` | difficile (FP élevés) |
| `require-exhaustive-switch` | Switch sur unions discriminées uniquement | difficile (Tsc backend) |
| `no-let-var` | Bannir `let` (améliorer `prefer-const`) | facile |
| `no-imperative-loops` | Ajouter while/do-while au ban | facile |
| `no-mutation-methods` | Ajouter plus de méthodes mutantes | moyen |
| `no-this-mutation` | Interdire mutation de `this` après constructeur | moyen |

### TokenMap-inspired (nouvelles)

| Règle | Description | Faisabilité |
|-------|-------------|-------------|
| `no-long-closure` | Arrow functions multi-lignes (seuil strict) | moyen |
| `callable-burden-hotspot` | Score composite (lignes × complexité) | difficile |
| `no-top-heavy-file` | Top 3 callables > X% du burden | difficile |

---

## Améliorations de règles existantes

### Cross-file à étendre

| Règle | Amélioration | Priorité |
|-------|--------------|----------|
| `no-identical-functions` | Alpha-rename normalization (variables locales) | basse |
| `inconsistent-function-call` | Support namespace imports (`ns.Foo()`) | basse |
| `data-clumps` | Étendre au cross-file | moyenne |
| `symmetric-pairs` | Étendre au cross-file (par feature directory) | moyenne |
| `arguments-order` | Comparer définition vs call-sites cross-file | haute |

### ImportIndex à étendre

| Langage | Status | Priorité |
|---------|--------|----------|
| Vue SFC | `<script setup>`, defineExpose/Props/Emits | moyenne |
| SQL | Dépendances cross-migrations | basse |
| tsconfig paths | Résolution des alias `@/...` | haute |

---

## Project-scope infrastructure

| Tâche | Description | Priorité |
|-------|-------------|----------|
| `ProjectCheck` trait | Nouveau backend pour règles cross-file | moyenne |
| Index call-sites namespace | Tracker `ns.Symbol()` pour namespace imports | basse |
| Transitive re-exports | Flattening des `export * from` | basse |

---

## e18e/eslint-plugin — Modernization & Performance

Plugin ESLint pour moderniser le code JS/TS. 20 règles analysées, 13 manquantes.

### Haute priorité

| Règle | Description | Catégorie |
|-------|-------------|-----------|
| `prefer-nullish-coalescing` | `x != null ? x : y` → `x ?? y` | typescript |
| `ban-dependencies` | Bannir lodash, moment, underscore → alternatives légères | imports |
| `prefer-static-regex` | Hisser regex hors fonctions (étendre react_hoist_regex) | performance |

### Moyenne priorité

| Règle | Description |
|-------|-------------|
| `prefer-array-to-reversed` | `[...arr].reverse()` → `arr.toReversed()` (ES2023) |
| `prefer-array-to-sorted` | `arr.slice().sort()` → `arr.toSorted()` (ES2023) |
| `prefer-array-to-spliced` | Idem pour splice (ES2023) |
| `prefer-array-fill` | `Array.from({length: n}, () => v)` → `Array(n).fill(v)` |
| `prefer-object-has-own` | `obj.hasOwnProperty(k)` → `Object.hasOwn(obj, k)` (ES2022) |
| `prefer-exponentiation-operator` | `Math.pow(x, y)` → `x ** y` |
| `prefer-url-canparse` | try-catch `new URL()` → `URL.canParse()` |
| `prefer-timer-args` | `setTimeout(() => fn(a), 100)` → `setTimeout(fn, 100, a)` |
| `prefer-array-from-map` | `[...iter].map(fn)` → `Array.from(iter, fn)` |
| `no-indexof-equality` | `str.indexOf('x') === 0` → `str.startsWith('x')` |

### À étendre (partielles)

| Règle comply | Extension |
|--------------|-----------|
| `prefer-array-some` | Ajouter `arr.find(pred) !== undefined` |
| `prefer-spread` | Ajouter `Object.assign({}, a, b)` et `fn.apply()` |

---

## Vertical Codebase (TkDodo) — Architecture par domaine

Règles pour forcer l'architecture verticale vs horizontale. Partiellement couvert par `layer-import-boundary` et `api-import-from-public-index`.

### Nouvelles règles

| Règle | Description | Faisabilité |
|-------|-------------|-------------|
| `no-horizontal-folders` | Bannir imports depuis `src/utils/*`, `src/hooks/*`, `src/types/*`, `src/components/*` | facile |
| `no-global-types-file` | Interdire `types.ts` à la racine ou partagés massivement | facile |
| `feature-boundary-strict` | Feature A ne peut importer Feature B que via `index.ts` | moyen (étend api-import-from-public-index) |
| `colocate-hook-with-component` | Hook spécifique à un composant doit vivre dans le même fichier/dossier | difficile |
| `no-barrel-re-export-all` | Interdire `export * from` dans les index.ts (masque les dépendances) | facile |

### Améliorations règles existantes

| Règle | Amélioration |
|-------|--------------|
| `api-import-from-public-index` | Configurable par feature (définir les "verticals" autorisées) |
| `layer-import-boundary` | Support des alias tsconfig (`@/domain`, `@/infra`) |

### Config suggérée (defaults.toml)

```toml
[rules.no-horizontal-folders]
banned_patterns = ["src/utils/*", "src/hooks/*", "src/types/*", "src/components/*"]
allowed_horizontal = ["src/design-system/*", "src/ui/*"]

[rules.feature-boundary-strict]
feature_roots = ["src/features/*", "src/modules/*"]
```

---

## Notes

- Voir `docs/hickey-rules-todo.md` pour détails Rich Hickey
- Voir `docs/project-scope-multi-lang.md` pour architecture multi-langage
- Voir `docs/sonar-candidates.md` pour 100/106 règles SonarJS implémentées
