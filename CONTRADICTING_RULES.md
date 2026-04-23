# Contradicting Rules Analysis

> Analyse des **924 règles** comply — conflits et redondances identifiés.



---

## 1. CONFLITS CRITIQUES (Mutuellement Exclusifs)

### 1.1 Export Style (3 règles)

| Rule                    | Category   | Description                                        |
| ----------------------- | ---------- | -------------------------------------------------- |
| `no-default-export`     | typescript | Default exports break tree-shaking and refactoring |
| `prefer-default-export` | imports    | Single export should be default                    |
| `no-named-export`       | imports    | Named exports forbidden                            |

**Conflit**: Impossible d'activer les 3.

**Recommandation**:
- **GARDER**: `no-default-export`
- **SUPPRIMER**: `prefer-default-export`, `no-named-export`

REVIEW: ok pour ta reco

---

### 1.2 Default Parameters (3 règles)

| Rule                        | Category     | Description                             |
| --------------------------- | ------------ | --------------------------------------- |
| `no-default-params`         | typescript   | Default params hide behavior            |
| `prefer-default-parameters` | unicorn      | Prefer default params over reassignment |
| `bool-param-default`        | code-quality | Boolean params should have defaults     |

**Conflit**: `no-default-params` interdit ce que les 2 autres encouragent.

**Recommandation**:

- **GARDER**: `prefer-default-parameters`
- **SUPPRIMER**: `no-default-params`, `bool-param-default`

REVIEW: ok pour ta reco

---

### 1.3 Boolean Flag Parameters (2 règles)

| Rule                    | Category     | Description                            |
| ----------------------- | ------------ | -------------------------------------- |
| `no-boolean-flag-param` | code-quality | Boolean flag params hide two behaviors |
| `bool-param-default`    | code-quality | Boolean params should have defaults    |

**Conflit**: L'un dit "pas de boolean params", l'autre dit "si boolean params, avec default".

**Recommandation**:
- **GARDER**: `no-boolean-flag-param`

- **SUPPRIMER**: `bool-param-default` (déjà marqué pour suppression au 1.2)

  REVIEW: ok pour ta reco

---

### 1.4 Array Reduce (2 règles)

| Rule                   | Category     | Description                  |
| ---------------------- | ------------ | ---------------------------- |
| `no-array-reduce`      | unicorn      | reduce() forbidden           |
| `reduce-initial-value` | code-quality | reduce() needs initial value |

**Conflit**: `reduce-initial-value` régule ce que `no-array-reduce` interdit.

**Recommandation**:
- **GARDER**: `no-array-reduce`

- **SUPPRIMER**: `reduce-initial-value`

  REVIEW: ok pour ta reco

---

### 1.5 Error Handling Philosophy (5 règles)

| Rule                               | Category     | Description               |
| ---------------------------------- | ------------ | ------------------------- |
| `no-throw`                         | code-quality | Never throw — use Result  |
| `no-try-statements`                | functional   | No try blocks             |
| `no-promise-reject`                | functional   | No Promise.reject()       |
| `jsdoc/require-throws`             | jsdoc        | Document @throws          |
| `jsdoc/require-throws-description` | jsdoc        | @throws needs description |

**Conflit MAJEUR**: Si `no-throw` actif, `jsdoc/require-throws` inutile.

**Recommandation** — choix architectural:
- **Style Result**: GARDER `no-throw`, `no-try-statements` → SUPPRIMER les règles jsdoc throws
- **Style Exception**: SUPPRIMER `no-throw`, `no-try-statements` → GARDER jsdoc throws

REVIEW: ok pour ta reco

---

### 1.6 TypeScript Enums (3 règles)

| Rule                          | Category   | Description                      |
| ----------------------------- | ---------- | -------------------------------- |
| `no-enum`                     | typescript | Never use enums                  |
| `ts-no-const-enum`            | typescript | No const enums (isolatedModules) |
| `ts-prefer-enum-initializers` | typescript | Enum values need init            |

**Conflit**: `no-enum` interdit ce que les autres régulent.

**Recommandation**:
- **GARDER**: `no-enum`, `ts-no-const-enum` (raisons différentes)

- **SUPPRIMER**: `ts-prefer-enum-initializers`

  REVIEW: ok pour ta reco

---

### 1.7 Namespace (2 règles)

| Rule                          | Category   | Description                  |
| ----------------------------- | ---------- | ---------------------------- |
| `ts-no-namespace`             | typescript | namespace is legacy          |
| `ts-prefer-namespace-keyword` | typescript | use `namespace` not `module` |

**Conflit**: L'un interdit namespace, l'autre dit comment l'écrire.

**Recommandation**:
- **GARDER**: `ts-no-namespace`

- **SUPPRIMER**: `ts-prefer-namespace-keyword`

  REVIEW: ok pour ta reco

---

### 1.8 Static Classes (2 règles)

| Rule                        | Category   | Description                               |
| --------------------------- | ---------- | ----------------------------------------- |
| `no-static-only-class`      | unicorn    | No class with only static members         |
| `ts-class-methods-use-this` | typescript | Methods not using `this` should be static |

**Conflit subtil**: Suivre `ts-class-methods-use-this` → tout devient static → `no-static-only-class` se déclenche.

**Recommandation**:

- **GARDER LES DEUX** — la solution est d'extraire en fonctions standalone

REVIEW: ok pour ta reco

---

### 1.9 Class Inheritance vs Error Definition (2 règles)

| Rule                      | Category   | Description                       |
| ------------------------- | ---------- | --------------------------------- |
| `no-class-inheritance`    | functional | No `extends` — prefer composition |
| `custom-error-definition` | unicorn    | Errors must extend Error properly |

**Conflit**: `no-class-inheritance` interdit `extends`, mais les custom errors en ont besoin.

**Recommandation**:
- **MODIFIER**: `no-class-inheritance` → exclure `extends Error`

REVIEW: ok pour ta reco, ignore les extends dès qu'il y a error dans le truc qu'on extend (donc il faut qu'on puisse aussi extends TaggedError)

---

### 1.10 Type Definitions (2 règles)

| Rule                             | Category   | Description                            |
| -------------------------------- | ---------- | -------------------------------------- |
| `ts-consistent-type-definitions` | typescript | Enforce interface OR type consistently |
| `prefer-type-over-interface`     | typescript | Prefer `type` unless extending         |

**Conflit**: `ts-consistent-type-definitions` peut forcer `interface`, `prefer-type-over-interface` veut `type`.

**Recommandation**:
- **GARDER**: `prefer-type-over-interface`

- **SUPPRIMER**: `ts-consistent-type-definitions`

  REVIEW: ok pour ta reco : il faut autoriser une interface uniquement si on fait un implements dessus

---

### 1.11 Null Usage (2 règles)

| Rule                       | Category | Description                        |
| -------------------------- | -------- | ---------------------------------- |
| `no-null`                  | unicorn  | Use undefined instead of null      |
| `node-no-callback-literal` | node     | Callbacks expect null as first arg |

**Conflit**: Node.js conventions utilisent `null`, mais `no-null` l'interdit.

**Recommandation**:
- **GARDER**: `no-null`
- **DÉSACTIVER PAR DÉFAUT**: `node-no-callback-literal`

REVIEW: ok pour ta reco

---

### 1.12 Module System (2 règles)

| Rule                 | Category | Description             |
| -------------------- | -------- | ----------------------- |
| `import-no-commonjs` | imports  | Forbids CommonJS        |
| `node-exports-style` | node     | Enforces CommonJS style |

**Conflit**: L'un interdit CommonJS, l'autre le régule.

**Recommandation**:
- **GARDER**: `import-no-commonjs`
- **DÉSACTIVER PAR DÉFAUT**: `node-exports-style`

REVIEW: ok pour ta reco, supprime node-exports-style 

---

### 1.13 Test Hooks (2 règles)

| Rule                  | Category | Description                   |
| --------------------- | -------- | ----------------------------- |
| `require-hook`        | testing  | Side effects must be in hooks |
| `playwright-no-hooks` | testing  | No hooks — use helpers        |

**Conflit contextuel**: Différentes philosophies selon le framework.

**Recommandation**:

- **GARDER LES DEUX** — scope par file pattern (`.spec.ts` vs `.test.ts`)

REVIEW: ok pour ta reco

---

## 2. REDONDANCES

### 2.1 Return Types (3 règles, 1 suffit)

| Rule                                | Scope                     |
| ----------------------------------- | ------------------------- |
| `explicit-return-type-on-exported`  | Exported functions only   |
| `ts-explicit-function-return-type`  | ALL functions             |
| `ts-explicit-module-boundary-types` | Exported functions + args |

**Recommandation**:
- **GARDER**: `ts-explicit-module-boundary-types`
- **SUPPRIMER**: `explicit-return-type-on-exported`
- **DÉSACTIVER PAR DÉFAUT**: `ts-explicit-function-return-type`

REVIEW : on garde ts-explicit-function-return-type, supprime les autres

---

### 2.2 File Header (2 règles, même chose)

| Rule                          | Description                       |
| ----------------------------- | --------------------------------- |
| `module-header`               | File must start with JSDoc header |
| `jsdoc/require-file-overview` | File must have @file tag          |

**Recommandation**:
- **GARDER**: `module-header`

- **SUPPRIMER**: `jsdoc/require-file-overview`

  REVIEW: supprime les 2

---

### 2.3 Barrel Files (2 règles liées)

| Rule                  | Description                     |
| --------------------- | ------------------------------- |
| `avoid-barrel-files`  | Barrel files hurt tree-shaking  |
| `avoid-re-export-all` | `export *` hides public surface |

**Pas une redondance** — complémentaires. Les deux peuvent rester.

REVIEW: ok pour ta reco

---

## 3. RÉSUMÉ

### Règles à SUPPRIMER (12)

| Rule                               | Raison                                             |
| ---------------------------------- | -------------------------------------------------- |
| `prefer-default-export`            | Contredit `no-default-export`                      |
| `no-named-export`                  | Trop restrictif                                    |
| `no-default-params`                | Contredit `prefer-default-parameters`              |
| `bool-param-default`               | Contredit `no-boolean-flag-param`                  |
| `reduce-initial-value`             | Inutile si `no-array-reduce` actif                 |
| `ts-prefer-enum-initializers`      | Inutile si `no-enum` actif                         |
| `ts-prefer-namespace-keyword`      | Inutile si `ts-no-namespace` actif                 |
| `ts-consistent-type-definitions`   | Contredit `prefer-type-over-interface`             |
| `explicit-return-type-on-exported` | Redondant avec `ts-explicit-module-boundary-types` |
| `jsdoc/require-file-overview`      | Redondant avec `module-header`                     |
| `jsdoc/require-throws`             | Inutile si `no-throw` actif (style Result)         |
| `jsdoc/require-throws-description` | Inutile si `no-throw` actif (style Result)         |

### Règles à DÉSACTIVER par défaut (4)

| Rule                               | Raison                        |
| ---------------------------------- | ----------------------------- |
| `node-exports-style`               | Legacy CommonJS               |
| `ts-explicit-function-return-type` | Trop strict pour code interne |
| `node-no-callback-literal`         | Legacy Node.js pattern        |
| `no-throw`                         | Opt-in (style Result)         |

### Règle à MODIFIER (1)

| Rule                   | Changement              |
| ---------------------- | ----------------------- |
| `no-class-inheritance` | Exclure `extends Error` |

### Décision architecturale requise (1)

**Error Handling Style**: Result/Either vs Exceptions?
- Si Result: garder `no-throw`, supprimer jsdoc throws
- Si Exceptions: supprimer `no-throw`, garder jsdoc throws

---

## 4. NEXT STEPS

1. [ ] Supprimer 12 règles marquées pour suppression
2. [ ] Mettre à jour `defaults.toml` pour désactiver 4 règles
3. [ ] Modifier `no-class-inheritance` pour exclure `extends Error`
4. [ ] Décider style de gestion d'erreurs (Result vs Exceptions)
5. [ ] Fix e2e test après cleanup
