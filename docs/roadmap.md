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

## Batch 4 — Performance

| ID | Title | Type | Status | Severity | Blocked by | Estimation |
|---|---|---|---|---|---|---|
| ISS-027 | prefilter_pass — pré-filtre par littéraux avant matching AST | feature | done | high | — | M |
