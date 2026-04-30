# Roadmap — Comply False Positives (Amadeo audit)

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
| ISS-010 | react-jsx-no-jsx-as-prop + règles pédantiques shadcn/Radix | enhancement | done | low | — | M |

## Batch 4 — Rust cross-fire & scope fixes (tokio audit)

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-063 | ts-no-magic-numbers fire sur fichiers Rust | bug | open | high | — | S |
| ISS-064 | ts-no-loop-func fire sur closures Rust | bug | open | high | — | S |
| ISS-065 | no-history-in-comments FP sur "was"/"previously" descriptif | bug | open | high | — | M |
| ISS-066 | comment-prose-quality sur code examples dans doc comments | bug | open | high | — | M |
| ISS-067 | inverted-assertion-arguments FP sur Rust assert_eq! | bug | open | medium | — | S |
| ISS-068 | boolean-naming FP sur noms idiomatiques Rust | enhancement | open | medium | — | S |
| ISS-069 | rust-no-mutex-in-single-threaded heuristique trop simpliste | bug | open | medium | — | M |

## Batch 5 — Nouvelles règles Rust

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-070 | rust-no-allow-without-reason : #[allow] sans justification | feature | open | medium | — | M |
| ISS-071 | rust-prefer-arc-clone : Arc::clone(&x) vs x.clone() | feature | open | low | — | M |
