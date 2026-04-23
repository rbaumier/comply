# Rules Roadmap

## Infrastructure manquante

### 1. ICU Message Format Parser
**Pour :** `i18n-json-valid-message-syntax`

```toml
# Cargo.toml
icu_messageformat_parser = "0.2"  # ou impl custom
```

**Fonctionnalités requises :**
- Parser `{variable}`
- Parser `{count, plural, one {...} other {...}}`
- Parser `{gender, select, male {...} female {...}}`
- Détecter accolades non fermées
- Valider syntaxe plural/select

---

### 2. JSON Cross-file Index (LocaleIndex)
**Pour :** `i18n-json-identical-keys`, `i18n-json-identical-placeholders`

**Design proposé :**
```rust
pub struct LocaleIndex {
    // Map: locale_dir -> { key -> [files_containing_key] }
    locales: HashMap<PathBuf, HashMap<String, Vec<PathBuf>>>,
}

impl LocaleIndex {
    pub fn build(json_files: &[&Path]) -> Self;
    pub fn get_missing_keys(&self, base_locale: &Path) -> Vec<MissingKey>;
    pub fn get_placeholder_mismatches(&self) -> Vec<PlaceholderMismatch>;
}
```

**Intégration :** Étendre `ProjectCtx` avec `locale_index()`.

---

### 3. Markdown Backend
**Pour :** `markdown-canonical-code-block-language`

**Options :**
1. Crate `pulldown-cmark` (parser MD)
2. Tree-sitter `tree-sitter-markdown`

**Design :**
```rust
// src/files.rs
Language::Markdown => tree_sitter_md::language(),

// Nouveau backend ou TextCheck avec regex sur ```lang
```

---

### 4. Package.json Cache
**Pour :** `import-no-extraneous-dependencies`

**Design :**
```rust
pub struct PackageJsonIndex {
    // Map: project_root -> parsed package.json
    packages: HashMap<PathBuf, PackageJson>,
}

pub struct PackageJson {
    dependencies: HashSet<String>,
    dev_dependencies: HashSet<String>,
    peer_dependencies: HashSet<String>,
}
```

---

## Règles à implémenter

### Faisables maintenant (10)

| Règle | Catégorie | Difficulté | Notes |
|-------|-----------|------------|-------|
| `html-no-nested-interactive` | a11y | Medium | AST walk, detect nested buttons/links |
| `html-no-skip-heading-levels` | a11y | Medium | State tracking during walk |
| `html-require-input-label` | a11y | Medium | Match input id with label for |
| `max-call-chain-depth` | code-quality | Easy | Count chained member_expression |
| `no-extra-arguments` | code-quality | Medium | Compare args vs params |
| `import-no-cycle` | imports | Medium | ImportIndex.get_importers() récursif |
| `require-path-exists` | imports | Easy | fs::exists sur import path |
| `no-unsafe-regex` | security | Hard | regex-syntax + complexity analysis |
| `functional-immutable-data` | functional | Hard | Cross-scope mutation tracking |
| `no-use-of-empty-return-value` | code-quality | Medium | Tsgolint backend |

### Après infra ICU (1)

| Règle | Catégorie | Prérequis |
|-------|-----------|-----------|
| `i18n-json-valid-message-syntax` | i18n | ICU parser |

### Après infra LocaleIndex (2)

| Règle | Catégorie | Prérequis |
|-------|-----------|-----------|
| `i18n-json-identical-keys` | i18n | LocaleIndex |
| `i18n-json-identical-placeholders` | i18n | LocaleIndex + ICU parser |

### Après infra Package.json (1)

| Règle | Catégorie | Prérequis |
|-------|-----------|-----------|
| `import-no-extraneous-dependencies` | imports | PackageJsonIndex |

### Après infra Markdown (1)

| Règle | Catégorie | Prérequis |
|-------|-----------|-----------|
| `markdown-canonical-code-block-language` | markdown | Markdown backend |

### React useEffect (heuristique) (3)

| Règle | Catégorie | Approche |
|-------|-----------|----------|
| `react-no-adjust-state-on-prop-change` | react | Heuristique: setState dans useEffect où dep ressemble à un prop |
| `react-no-pass-data-to-parent` | react | Heuristique: appel de callback dans useEffect |
| `react-no-reset-all-state-on-prop-change` | react | Heuristique: multiple setState sur dep id-like |

---

## Ordre d'implémentation suggéré

### Phase 1 : Règles sans infra (priorité haute)
1. `html-no-nested-interactive`
2. `html-no-skip-heading-levels`
3. `html-require-input-label`
4. `max-call-chain-depth`
5. `no-extra-arguments`
6. `import-no-cycle`
7. `require-path-exists`

### Phase 2 : Règles complexes
8. `no-unsafe-regex` (ReDoS)
9. `functional-immutable-data`
10. `no-use-of-empty-return-value` (Tsgolint)

### Phase 3 : Infra + règles
11. Implémenter ICU parser
12. `i18n-json-valid-message-syntax`
13. Implémenter LocaleIndex
14. `i18n-json-identical-keys`
15. `i18n-json-identical-placeholders`

### Phase 4 : React heuristiques
16. `react-no-adjust-state-on-prop-change`
17. `react-no-pass-data-to-parent`
18. `react-no-reset-all-state-on-prop-change`

### Phase 5 : Nouveaux backends
19. Implémenter PackageJsonIndex
20. `import-no-extraneous-dependencies`
21. Implémenter Markdown backend
22. `markdown-canonical-code-block-language`

---

## Récap session

**Règles implémentées aujourd'hui : ~79**
- Tests : 2940 → 5620 (+2680)
- Catégories : Security, Async, Errors, React, Imports, TypeScript, Database, HTML/A11y, Code Quality, Zod, FP, XState, Naming, Performance, Tailwind, TOML
