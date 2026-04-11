# comply — Remaining Rules

Rules not yet implemented. The catalog (`comply catalog --json`) is the source of truth for what ships today.

---

## Tier 3 — Needs type info (tsc pipeline)

Requires a TypeScript type-aware pass (`comply typecheck` subcommand shelling out to `tsc --noEmit`).

| Rule | Backend | Approach |
|------|---------|----------|
| `strict-typing` — no inferred `any` | tsc | Filter codes 7005, 7006, 7031, 7034 |
| `option-vs-result` — `findUser` → `Option<User>` | tsc | Signature heuristic on `find*`/`get*` verbs |
| `misleading-name` — `userList: Set<User>` | tsc | Name suffix vs declared type |
| `data-clumps` — same 3+ fields in 2+ types | tsc | Cross-file structural match |
| `boundary-condition` — unchecked `arr[0]` / `arr.length - 1` | tsc | `noUncheckedIndexedAccess` off → emit |
| `no-raw-db-entity-in-handler` — handler returning Prisma entity | tsc | Match against `@prisma/client` types |
| `structured-api-error` — errors need `{type,code,status,detail}` | tsc | Shape match |
| `api-first` — handler without zod/openapi schema alongside | text | Filesystem cross-reference |

---

## Tier 5 — LLM / review-only (remaining)

9 LLM rules ship today: `llm-comment-quality`, `llm-intent-naming`, `llm-pii-in-logs`, `llm-function-abstraction-levels`, `llm-define-errors-out-of-existence`, `llm-pull-complexity-downward`, `llm-barricade-pattern`, `llm-temporal-decomposition`, `llm-shallow-module`.

These are NOT yet covered:

| Rule | Source |
|------|--------|
| Parse, don't validate | Philosophy |
| Make invalid states unrepresentable | Philosophy |
| Functional core, imperative shell | Philosophy |
| Document impossible states | Error Handling |
| Bound every input (reject at boundary) | Data |
| Crosscutting via wrapping (`withTracing`) | Architecture |
| Map DB entities to DTOs | Architecture |
| Error messages as step-by-step remediation | Project Hygiene |

---

## Tier 6 — Architectural / cross-project

`llm-temporal-decomposition` and `llm-shallow-module` now cover temporal decomposition and module depth. Remaining:

| Rule | Source |
|------|--------|
| Reuse before creating | Philosophy |
| Rule of Three | Philosophy |
| Prefer boring technology | Philosophy |
| DRY (repo-wide) | Philosophy |
| Vertical slices | Architecture |
| Shotgun Surgery | Architecture |
| Divergent Change | Architecture |
| Information leakage | Architecture |
| SRP per function/module | Functions |
| CQS — command OR query | Functions |
| Composition over inheritance | Functions |
| Tests/linting/CI/CD from day 1 | Project Hygiene |
| Constrain first, relax later | Project Hygiene |
| Codebase homogeneity | Project Hygiene |
| Structural guardrails over discipline | Project Hygiene |
| Hard cutover on migrations | Project Hygiene |
| Pin all versions | Project Hygiene |
| Group tests by feature, not type | File Structure |

---

## eslint-plugin-unicorn — non-implementable rules

Rules requiring capabilities comply does not have yet (scope analysis, per-module config, advanced regex parsing).

| Rule | Reason | Pre-requisite |
|------|--------|---------------|
| `better-regex` | Needs regex parsing + optimization engine | `regex-syntax` crate + optimizer |
| `consistent-function-scoping` | Needs scope analysis (variable capture detection) | Scope analysis infra |
| `isolated-functions` | Same as `consistent-function-scoping` | Scope analysis infra |
| `import-style` | Per-module config (default/namespace/named) | Config per-module in comply.toml |
| `no-unnecessary-polyfills` | Needs browserslist + polyfill DB | Browserslist integration |
| `no-unused-properties` | Needs whole-program data flow analysis | Whole-program analysis |
| `string-content` | User-configurable string pattern replacements | Config in comply.toml |

---

## eslint Plugins — remaining to evaluate

| Plugin | Notes |
|--------|-------|
| [eslint-plugin-package-json](https://github.com/michaelfaith/eslint-plugin-package-json) | Validate package.json structure |
| [eslint-plugin-better-tailwindcss](https://github.com/schoero/eslint-plugin-better-tailwindcss) | Tailwind class ordering/validation |
| [eslint-plugin-hexagonal-architecture](https://github.com/CodelyTV/eslint-plugin-hexagonal-architecture) | Enforce hex arch boundaries |
| [write-good](https://github.com/btford/write-good) | Prose quality in comments/docs |
| [eslint-plugin-no-secrets](https://github.com/nickdeis/eslint-plugin-no-secrets) | Detect hardcoded secrets (entropy-based) |
| [eslint-plugin-xss](https://github.com/Rantanen/eslint-plugin-xss) | XSS prevention |

### Already evaluated & implemented

- ✅ eslint-plugin-unicorn (131 rules)
- ✅ eslint-plugin-n (7 rules)
- ✅ typescript-eslint (14 rules)
- ✅ eslint-plugin-react (15 rules)
- ✅ eslint-plugin-react-refresh (1 rule)
- ✅ eslint-plugin-import (11 rules)
- ✅ eslint-plugin-regexp (10 rules)
- ✅ eslint-plugin-functional (3 rules)
- ✅ eslint-plugin-security (4 rules)
- ✅ eslint-plugin-no-unsanitized (patterns added)
- ✅ eslint-plugin-jsdoc (7 rules)
- ✅ eslint-plugin-playwright (10 rules)
- ✅ eslint-plugin-de-morgan (1 rule)
- ✅ eslint-plugin-simple-import-sort (covered by existing rules)
- ✅ eslint-plugin-jsx-a11y (33 rules — separate commit)
