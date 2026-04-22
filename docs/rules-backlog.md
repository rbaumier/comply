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

## Notes

- Voir `docs/hickey-rules-todo.md` pour détails Rich Hickey
- Voir `docs/project-scope-multi-lang.md` pour architecture multi-langage
- Voir `docs/sonar-candidates.md` pour 100/106 règles SonarJS implémentées
