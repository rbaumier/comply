# Comply — TODO

**État au 2026-04-26** · 1420 règles · 7804 tests

---

## 1. Nouvelles règles à implémenter

| Règle | Difficulté | Notes |
|-------|-----------|-------|
| `vertical-slice-no-role-folders` | Moyen | Détecte `src/services/` + `src/repositories/` + `src/handlers/` en parallèle |
| `single-call-site-inline` | Difficile | Fonction exportée référencée dans 1 seul fichier — ImportIndex cross-file |

---

## 3. Faux AstCheck → vrai AstCheck

~37 backends déclarés `AstCheck` ignorent l'AST et scannent `ctx.source.lines()`. En plus, 261 backends `TextCheck` explicites. Détail dans `TEXTCHECK_TO_ASTCHECK.md`.

---

## 4. Contradicting rules

Analyse dans `CONTRADICTING_RULES.md`. Vérifier que les recommandations "SUPPRIMER" sont appliquées.

---

## 5. Tooling

### `comply catalog`
Commande qui génère un markdown/HTML depuis les structs `RuleMeta` (id, description, remediation, severity, backend). Pas hand-maintained.

### Wire orphan `rust.rs`
Quelques fichiers `src/rules/*/rust.rs` avec un vrai `Check` impl ne sont pas branchés dans le `register()` de leur `mod.rs`. ~10 règles Rust supplémentaires gratuites.

---

## 6. Tiers futurs

### Tier 3 — Règles nécessitant `tsc` (8 règles)

Requiert une subcommand `comply typecheck` avec `tsc --noEmit`.

| Règle | Approche |
|-------|----------|
| `strict-typing` | Filter TS error codes 7005, 7006, 7031, 7034 |
| `option-vs-result` | Heuristic sur `find*`/`get*` verbs |
| `misleading-name` | Name suffix vs declared type |
| `data-clumps` | Cross-file structural match |
| `boundary-condition` | `noUncheckedIndexedAccess` off → emit |
| `no-raw-db-entity-in-handler` | Match against `@prisma/client` types |
| `structured-api-error` | Shape match `{type,code,status,detail}` |
| `api-first` | Handler sans zod/openapi schema adjacent |

### Tier 5 — LLM review-only (10 règles)

| Règle | Source |
|-------|--------|
| `parse-dont-validate` | Philosophy |
| `make-invalid-states-unrepresentable` | Philosophy |
| `functional-core-imperative-shell` | Philosophy |
| `document-impossible-states` | Error Handling |
| `bound-every-input` | Data |
| `crosscutting-via-wrapping` | Architecture |
| `map-db-entities-to-dtos` | Architecture |
| `error-messages-as-step-by-step-remediation` | Project Hygiene |
| `command_injection_review` | Security — taint analysis LLM |
| `path_traversal_review` | Security — taint analysis LLM |

### Tier 6 — Architectural / cross-project LLM (18 règles)

`reuse-before-creating`, `rule-of-three`, `prefer-boring-technology`, `dry-repo-wide`, `vertical-slices`, `shotgun-surgery`, `divergent-change`, `information-leakage`, `srp-per-function-module`, `cqs-command-or-query`, `composition-over-inheritance`, `tests-linting-ci-cd-from-day-1`, `constrain-first-relax-later`, `codebase-homogeneity`, `structural-guardrails-over-discipline`, `hard-cutover-on-migrations`, `pin-all-versions`, `group-tests-by-feature-not-type`

### Unicorn — non-implémentables aujourd'hui (7 règles)

| Règle | Prérequis |
|-------|-----------|
| `better-regex` | `regex-syntax` crate + optimizer |
| `consistent-function-scoping` | Scope analysis (variable capture detection) |
| `isolated-functions` | Idem |
| `import-style` | Config per-module dans `comply.toml` |
| `no-unnecessary-polyfills` | Browserslist + polyfill DB |
| `no-unused-properties` | Whole-program data-flow analysis |
| `string-content` | Config user-defined dans `comply.toml` |

---

## 7. Suggestions non triées

- serde/zod cross-pollination : règles serde → zod et vice-versa (ex: `rust-serde-deny-unknown-fields` ↔ `.strict()` zod)
- Nesting catastrophique Rust → early-return refactor (AST check `nesting depth >= 4` + présence `if let Some`)
