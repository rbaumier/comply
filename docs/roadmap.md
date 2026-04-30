# Roadmap — Comply

## Bug fixes & FP reduction

### Priorité haute

| ID | Titre | Status |
|----|-------|--------|
| ISS-001 | unused-file & dead-export : entry points framework manquants | open |
| ISS-003 | no-side-effects-in-initialization sur fichiers de test | open |
| ISS-005 | no-hardcoded-secret FP sur JSX et formulaires | partiel — JSX et i18n encore FP |
| ISS-010 | react-jsx-no-jsx-as-prop FP sur patterns shadcn/Radix | open |
| ISS-030 | prefer-promise-all FP avec destructuring bindings | open |
| ISS-052 | no-useless-intersection remediation dangereuse pour `& any` | open |
| ISS-056 | react-passive-event-listeners recommande passive quand preventDefault est utilisé | open |

### Priorité moyenne

| ID | Titre | Status |
|----|-------|--------|
| ISS-002 | Règles techno-spécifiques sans vérification framework | partiel — playwright fixé, reste rn + import-dynamic |
| ISS-007 | no-generic-names trop agressif | partiel — skip tests/stories ok, scope exceptions à faire |
| ISS-008 | no-extraneous-import / no-default-export sans exception config | open |
| ISS-009 | tailwind-classnames-order doublon avec prettier plugin | open |
| ISS-029 | new-for-builtins FP sur identifiants shadowés | open |
| ISS-031 | rust-thread-sleep-in-async FP sur tokio::time::sleep | open |
| ISS-033 | prefer-spread FP sur Array.from avec mapper, String.concat | open |
| ISS-039 | rust-select-without-biased FP sur futures::select! | open |
| ISS-040 | ts-no-promise-void-function-misuse à réécrire en AST | open |
| ISS-042 | sql-no-union-when-union-all heuristique id trop faible | open |
| ISS-043 | no-submit-handler-without-preventDefault FN receiver + nesting | open |
| ISS-046 | tanstack-query-pass-signal-to-fetch FN sur ctx.signal | open |
| ISS-047 | react-no-cookies-in-layout FP — pas de check import next/headers | open |
| ISS-049 | vue-component-pascal-case FN — tags lowercase custom passent | open |
| ISS-050 | xstate-no-inline-implementation FP — pas de check createMachine | open |
| ISS-053 | security-require-rate-limit-auth FP — middlewares globaux non reconnus | open |
| ISS-055 | rn-no-inline-renderitem FP — pas de check FlatList/SectionList | open |
| ISS-058 | i18n-json-valid-message-syntax — supporter i18next nativement | open |
| ISS-059 | ts-no-extraneous-class flagge les classes décorées (voulu) | open |

### Priorité basse

| ID | Titre | Status |
|----|-------|--------|
| ISS-037 | vitest-no-disabled-tests FN sur test dirs non standard | open |
| ISS-044 | security-bcrypt-min-rounds FN — ne suit pas les const | open |
| ISS-051 | import-consistent-type-specifier-style remediation incomplète | open |
| ISS-054 | no-logger-in-business-logic FN — macros de log importées | open |

## Features plateforme

| ID | Titre | Status |
|----|-------|--------|
| ISS-026 | Capacités plateforme (autofix, reports, config extends, init) | partiel — prefilter done |

## Nouvelles règles (backlog)

| ID | Catégorie | Nb règles |
|----|-----------|-----------|
| ISS-011 | Sécurité | 90 |
| ISS-012 | JavaScript / Fondamentaux | 88 |
| ISS-013 | TypeScript | 86 |
| ISS-014 | React | 23 |
| ISS-015 | Vue / Nuxt | 64 |
| ISS-016 | Tests | 20 |
| ISS-017 | Architecture / Complexité / Nommage | 29 |
| ISS-018 | Prose / Commentaires | 27 |
| ISS-019 | HTML / SEO + Accessibilité | 47 |
| ISS-020 | CSS / Tailwind + Package.json | 35 |
| ISS-021 | GitHub Actions | 38 |
| ISS-022 | Dockerfile + Kubernetes | 42 |
| ISS-023 | Environnement + SQL / ORM | 36 |
| ISS-024 | Rust + IaC + CI/CD | 19 |
| ISS-025 | OpenAPI + Git/FS + Performance + i18n | 35 |

## Résolu (supprimé du tracker)

ISS-004, ISS-006, ISS-027, ISS-028, ISS-032, ISS-034, ISS-035, ISS-036, ISS-038, ISS-041, ISS-045, ISS-048, ISS-057.
