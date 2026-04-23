# Règles typescript-eslint — Décisions finales

Règles disponibles via `oxlint --type-aware`. Classées par décision.

---

## À AJOUTER — 52 règles

### Bugs & Sécurité (18)
| Règle | Description |
|-------|-------------|
| `no-explicit-any` | Interdit `any` explicite |
| `no-unsafe-declaration-merging` | Fusions interface/class dangereuses |
| `no-unsafe-enum-comparison` | Comparaison enum avec non-enum |
| `no-unsafe-function-type` | Type `Function` non typé |
| `no-unsafe-unary-minus` | `-x` sur non-number |
| `no-implied-eval` | `setTimeout("code")` = eval caché |
| `no-base-to-string` | `.toString()` sur objets sans override |
| `no-confusing-void-expression` | void dans contexte valeur |
| `no-misused-spread` | Spread sur type incompatible |
| `use-unknown-in-catch-callback-variable` | catch(e) → unknown |
| `strict-void-return` | Retour void strict |
| `no-deprecated` | APIs @deprecated |
| `prefer-promise-reject-errors` | reject() avec Error |
| `no-meaningless-void-operator` | void sans effet |
| `no-redundant-type-constituents` | `string \| "foo"` redondant |
| `no-duplicate-enum-values` | Enum avec valeurs dupliquées |
| `no-duplicate-type-constituents` | `A \| A` dupliqué |
| `no-misused-new` | new/constructor mal défini |

### Qualité de code (10)
| Règle | Description |
|-------|-------------|
| `no-non-null-asserted-nullish-coalescing` | `x! ?? y` contradictoire |
| `no-useless-empty-export` | `export {}` sans effet |
| `no-invalid-void-type` | void comme type de variable |
| `related-getter-setter-pairs` | getter/setter types incompatibles |
| `unified-signatures` | Overloads → signature unique |
| `no-unnecessary-type-parameters` | Generic `<T>` inutile |
| `no-unnecessary-boolean-literal-compare` | `x === true` → `x` |
| `no-unnecessary-template-expression` | `` `${x}` `` inutile |
| `no-unnecessary-type-constraint` | `<T extends unknown>` inutile |
| `no-unnecessary-type-conversion` | `String(x)` sur string |

### Optimisations (8)
| Règle | Description |
|-------|-------------|
| `no-unnecessary-parameter-property-assignment` | `this.x = x` redondant |
| `no-inferrable-types` | `const x: number = 5` inférable |
| `no-wrapper-object-types` | `String` → `string` |
| `prefer-regexp-exec` | `.match()` → `.exec()` |
| `prefer-string-starts-ends-with` | `.indexOf() === 0` → `.startsWith()` |
| `prefer-return-this-type` | Retourner `this` |
| `dot-notation` | `obj["prop"]` → `obj.prop` |
| `consistent-return` | Toutes branches retournent ou aucune |

### Style (10)
| Règle | Description |
|-------|-------------|
| `array-type` | Forcer `T[]` |
| `consistent-type-imports` | `import type` pour types |
| `consistent-generic-constructors` | `new Map<K,V>()` explicite |
| `consistent-indexed-object-style` | `Record<string, T>` |
| `prefer-as-const` | `"foo" as const` |
| `prefer-for-of` | `for...of` au lieu de `for(i)` |
| `prefer-function-type` | `() => T` au lieu de `{ (): T }` |
| `class-literal-property-style` | `readonly x = 5` |
| `parameter-properties` | `constructor(public x)` |
| `explicit-function-return-type` | Types retour obligatoires |

### Restrictions (6)
| Règle | Description |
|-------|-------------|
| `ban-ts-comment` | Interdit @ts-ignore, @ts-nocheck |
| `ban-types` | Interdit Object, {}, Function, String, Number, Boolean |
| `no-namespace` | Interdit namespaces TS |
| `no-require-imports` | Interdit require() |
| `no-var-requires` | Interdit const x = require() |
| `no-import-type-side-effects` | import type sans side effects |
| `triple-slash-reference` | Interdit /// <reference> |
| `no-empty-interface` | Interdit interface vide |
| `no-empty-object-type` | Interdit {} comme type |
| `no-extraneous-class` | Interdit classes inutiles |
| `no-this-alias` | Interdit const self = this |
| `explicit-module-boundary-types` | Types explicites sur fonctions |

---

## SKIP — 21 règles

| Règle | Raison |
|-------|--------|
| `no-useless-default-assignment` | On veut être explicite avec undefined |
| `no-unnecessary-type-arguments` | On veut l'inverse (forcer explicite) |
| `no-unnecessary-qualifier` | On ban les enum |
| `prefer-reduce-type-parameter` | On ban reduce |
| `non-nullable-type-assertion-style` | On ban les assertions |
| `prefer-readonly` | Trop verbeux |
| `prefer-readonly-parameter-types` | Trop verbeux |
| `consistent-type-assertions` | On interdit toutes les assertions |
| `consistent-type-definitions` | Déjà couvert par prefer-type-over-interface |
| `prefer-namespace-keyword` | On ban namespace |
| `prefer-enum-initializers` | On ban enum |
| `prefer-literal-enum-member` | On ban enum |
| `adjacent-overload-signatures` | On ban overloads |
| `ban-tslint-comment` | Legacy, pas pertinent |
| `no-restricted-types` | Redondant avec ban-types |
| `prefer-ts-expect-error` | On ban @ts-ignore et @ts-expect-error |

---

## DÉJÀ DANS COMPLY (tree-sitter) — 8 règles

| Règle oxlint | Règle comply existante |
|--------------|------------------------|
| `no-confusing-non-null-assertion` | `ts-no-confusing-non-null-assertion` |
| `no-extra-non-null-assertion` | `ts-no-extra-non-null-assertion` |
| `no-non-null-asserted-optional-chain` | `ts-no-non-null-asserted-optional-chain` |
| `no-non-null-assertion` | `ts-no-non-null-assertion` |
| `no-unnecessary-type-assertion` | `no-unnecessary-type-assertion` |
| `no-unsafe-type-assertion` | `typescript/no-unsafe-type-assertion` |
| `no-dynamic-delete` | `ts-no-dynamic-delete` |
| `no-array-delete` | `no-array-delete` |

---

## RÈGLES CUSTOM À CRÉER

| Règle | Description |
|-------|-------------|
| `require-type-arguments` | Forcer les type arguments explicites (inverse de no-unnecessary-type-arguments) |
| `no-type-assertion` | Interdire tout `as T` (pas juste les unsafe) |

---

## Résumé

| Catégorie | Nombre |
|-----------|--------|
| À ajouter | 52 |
| Skip | 21 |
| Déjà dans comply | 8 |
| Custom à créer | 2 |
| **Total** | **83** |
