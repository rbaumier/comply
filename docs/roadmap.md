# Roadmap — Comply False Positives (Amadeo audit)

## Review notes

- 2026-04-30 — Toutes les issues `done` listées dans cette roadmap ont été revues avec leur fiche `docs/issues` quand elle existe.
- Les fiches manquantes `ISS-004` et `ISS-006` ont été créées pour que les entrées `done` de la roadmap aient une trace dans `docs/issues`.
- Les fiches `ISS-059` à `ISS-062` sont explicitement marquées résolues dans `docs/issues` et sont maintenant reprises dans la roadmap.

## Batch 1 — Framework detection & graph (high impact)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-001 | unused-file & dead-export : entry points framework manquants | bug | done | critical | — | L |
| ISS-002 | Règles techno-spécifiques sans vérification framework | bug | done | high | — | M |
| ISS-003 | no-side-effects-in-initialization sur fichiers de test | bug | done | high | — | S |
| ISS-008 | no-extraneous-import / no-default-export sans exception config | bug | done | medium | — | S |

## Batch 2 — Heuristics refinement

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-004 | explicit-module-boundary-types doublon de explicit-function-return-type | enhancement | done | medium | — | M |
| ISS-005 | no-hardcoded-secret FP sur JSX et formulaires | bug | done | medium | — | M |
| ISS-007 | no-generic-names trop agressif sur patterns idiomatiques | enhancement | done | medium | — | M |
| ISS-006 | no-timing-attack FP sur validation client-side | bug | done | low | — | S |

## Batch 3 — Polish & ecosystem awareness

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-009 | tailwind-classnames-order doublon avec prettier plugin | enhancement | done | low | — | S |
| ISS-010 | react-jsx-no-jsx-as-prop : patterns shadcn/Radix | enhancement | done | low | — | M |

## Batch 3b — Recettage post-merge worktree-fp-hunting

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-059 | ts-no-extraneous-class : classes décorées | bug | done | high | — | S |
| ISS-060 | Classification fichiers : .d.tsx et .d.cts non skippés | bug | done | medium | — | S |
| ISS-061 | TUI : panic possible sur highlight_cache index out-of-bounds | bug | done | high | — | S |
| ISS-062 | playwright-no-page-pause : gating incohérent sur tests non-Playwright | bug | done | medium | — | S |

## Batch 4 — Rust cross-fire & scope fixes (tokio audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-063 | ts-no-magic-numbers fire sur fichiers Rust | bug | done | high | — | S |
| ISS-064 | ts-no-loop-func fire sur closures Rust | bug | done | high | — | S |
| ISS-065 | no-history-in-comments FP sur "was"/"previously" descriptif | bug | done | high | — | M |
| ISS-066 | comment-prose-quality sur code examples dans doc comments | bug | done | high | — | M |
| ISS-067 | inverted-assertion-arguments FP sur Rust assert_eq! | bug | done | medium | — | S |
| ISS-068 | boolean-naming FP sur noms idiomatiques Rust | enhancement | done | medium | — | S |
| ISS-069 | rust-no-mutex-in-single-threaded heuristique trop simpliste | bug | done | medium | — | M |

## Batch 5 — Nouvelles règles Rust

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-070 | rust-no-allow-without-reason : #[allow] sans justification | feature | done | medium | — | M | medium | — | M |
| ISS-071 | rust-prefer-arc-clone : Arc::clone(&x) vs x.clone() | feature | done | low | — | M | low | — | M |

## Batch 6 — TS test-awareness & crash fix (zustand + trpc audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-072 | api-response-envelope-consistency crash sur emoji UTF-8 | bug | done | critical | — | S | critical | — | S |
| ISS-073 | testing-no-undefined-mock-var FP sur spy-only mocks | bug | done | medium | — | M | medium | — | M |
| ISS-074 | no-undefined-argument FP dans les matchers d'assertion | bug | done | medium | — | S | medium | — | S |
| ISS-075 | unused-component-prop FP sur fichiers type-test | bug | done | medium | — | S | medium | — | S |
| ISS-076 | no-property-mutation trop strict dans les tests | bug | done | medium | — | M | medium | — | M |
| ISS-077 | consistent-function-scoping FP dans callbacks de test | enhancement | done | low | — | S | low | — | S |

## Batch 7 — Library & monorepo awareness (redux-toolkit audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-078 | dead-export/unused-file FP sur API publique de bibliothèque | bug | done | high | — | L |
| ISS-079 | no-implicit-deps ne résout pas les workspace deps (monorepo) | bug | done | high | — | M |
| ISS-080 | filename-naming-convention impose kebab-case sur composants React | bug | done | medium | — | S |
| ISS-081 | exports-last FP sur inline export const/function | bug | done | medium | — | S |
| ISS-082 | file-extension-in-import FP quand bundler détecté | bug | done | medium | — | M |

## Batch 8 — Data-processing & Cargo workspace awareness (polars audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-083 | explicit-units FP sur noms standard Rust (length, offset) | bug | done | medium | — | S |
| ISS-084 | no-type-encoded-names FP sur préfixes domaine (str_, arr_) | bug | done | medium | — | S |
| ISS-085 | rust-unused-dep ne gère pas les deps feature-gated | bug | done | high | — | M |
| ISS-086 | rust-no-as-numeric-cast trop strict pour data-processing | bug | done | high | — | M |
| ISS-087 | rust-pub-enum-without-non-exhaustive sur enums workspace-internal | bug | done | low | — | S |

## Batch 9 — Test-file awareness & crash fixes (swr + zod audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-088 | assertions-in-tests / playwright-expect-expect FP sur testing-library | bug | open | high | — | M |
| ISS-089 | no-test-return-statement FP sur return dans fonctions imbriquées | bug | open | high | — | S |
| ISS-090 | Règles UI/a11y/tailwind ne devraient pas fire sur JSX de test | bug | open | high | — | M |
| ISS-091 | Crash UTF-8 byte-indexing systémique (2+ règles touchées) | bug | open | critical | — | M |

## Batch 10 — JS/CJS awareness & module system (fastify audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-092 | no-unsafe-* rules ne devraient pas fire sur .js purs (36k FPs) | bug | open | critical | — | S |
| ISS-093 | import-no-commonjs / prefer-module doivent respecter "type" field | bug | open | high | — | M |
| ISS-094 | data-clumps FP sur signatures d'API framework | bug | open | medium | — | M |
| ISS-095 | require-hook flagge les require() d'import comme side effects | bug | open | medium | — | S |

## Batch 11 — Framework routing & JSDoc-as-types (svelte-kit audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-096 | filename-naming-convention FP sur conventions SvelteKit (+page) | bug | open | high | — | S |
| ISS-097 | jsdoc-needs-description FP sur annotations type-only | bug | open | high | — | S |
| ISS-098 | node-no-sync FP sur scripts de build et CLI | bug | open | medium | — | S |

## Batch 12 — Relaxed directories & framework internals (axum audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-099 | timeout-on-io FP sur framework internals et test clients | bug | open | medium | — | M |
| ISS-100 | Règles trop strictes dans examples/ et benches/ | enhancement | open | medium | — | M |

## Batch 13 — Crash fixes (date-fns & playwright)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-091 | Crash UTF-8 byte-indexing systémique (3+ règles confirmées) | bug | open | critical | — | M |
| ISS-101 | Stack overflow sur projets volumineux (playwright) | bug | open | high | — | M |

## Batch 14 — Decorator/DI framework awareness (nest audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-102 | ts-no-extraneous-class ne détecte pas decorators sur export class | bug | open | high | — | S |
| ISS-103 | parameter-properties FP sur injection de dépendances | bug | open | high | — | M |
| ISS-104 | ts-class-methods-use-this FP sur méthodes décorées | bug | open | high | — | M |
| ISS-105 | no-async-without-await FP sur handlers contractuels | bug | open | medium | — | M |
| ISS-106 | no-class-inheritance FP sur extension points de framework | bug | open | medium | — | S |

## Batch 15 — Playwright scope & test-component awareness (jotai audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-107 | Toutes les règles playwright-* fire sur vitest/jest | bug | open | high | — | M |
| ISS-108 | function-component-definition FP sur composants de test inline | bug | open | medium | — | S |
| ISS-109 | Comply timeout/hang sur projets >1000 fichiers TS | bug | open | high | ISS-101 | L |

## Batch 16 — Framework-gated rules & routing conventions (formik audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-110 | shadcn-* rules fire sans shadcn/radix installé | bug | open | high | — | M |
| ISS-111 | filename-naming-convention FP sur conventions Next.js routing | bug | open | medium | ISS-096 | S |

## Batch 17 — Binary crate & test-string awareness (starship audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-112 | no-duplicate-string FP dans tests Rust inline #[cfg(test)] | bug | open | high | — | S |
| ISS-113 | rust-impl-debug-on-public-types FP sur crates binaires | bug | open | medium | — | S |

## Batch 18 — Test struct awareness & Rust conventions (clap audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-114 | rust-partial-eq-without-eq FP sur structs de test | bug | open | medium | — | S |
| ISS-115 | filename-naming-convention FP sur exemples binaires Rust | bug | open | low | — | S |
| ISS-116 | id-length trop strict sur paramètres de closure Rust | bug | open | high | — | S |

## Batch 19 — Reactive framework awareness (solid audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-117 | no-side-effects-in-initialization FP sur frameworks réactifs | bug | open | high | — | M |

## Batch 20 — Config/routing default exports & env validation (create-t3-app audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-118 | no-default-export FP sur fichiers config et routing Next.js | bug | open | high | — | S |
| ISS-119 | zod-validate-env-at-startup FP sur le fichier de validation env | bug | open | high | — | S |
