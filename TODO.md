# comply — Remaining Rules

Rules not yet implemented. The catalog (`comply catalog --json`) is the source of truth for what ships today.

---

## Tier 2 — Remaining

| Rule | Backend | Notes |
|------|---------|-------|
| `symmetric-pairs` — `getFoo`/`setFoo`, `addX`/`removeX` | tree-sitter | Cross-reference exports |
| `pure-by-default` — no top-level mutable state reference | tree-sitter | Track top-level `let` + inner references |
| `intermediate-variables` — 2+ ops inside arg/return | tree-sitter | Count operator depth in arguments |
| `colocated-tests` — `foo.ts` needs `foo.test.ts` nearby | text | Filesystem check |

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

## Tier 4 — Heuristic / partial detection

| Rule | Backend | Notes |
|------|---------|-------|
| `justify-inaction` — empty catch/else without comment | tree-sitter | Empty block + missing preceding comment |
| `no-logger-in-business-logic` — `logger.info` in service/ | tree-sitter | Path-aware: flag in `services/`, `domain/`, `core/` |
| `auth-on-mutation` — create/update/delete handler needs auth helper | tree-sitter | Cross-ref call graph |
| `blank-line-between-blocks` — setup/validate/transform/return | text | Whitespace check (formatting) |
| `error-message-is-remediation` — error strings need a verb | text | Sentence heuristic on `new Error(...)` |
| `no-hidden-control-flow` — 3+ decorators stacked | tree-sitter | Count decorator nodes per function |
| `factory-di-shape` — `create*` fns should take deps object | tree-sitter | AST shape on `create*` exports |

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

## Future: Plugins

- [eslint-plugin-jsx-a11y](https://github.com/jsx-eslint/eslint-plugin-jsx-a11y)

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

# eslint Plugins to suppoort/evaluate
- https://github.com/jsx-eslint/eslint-plugin-jsx-a11y
- https://github.com/jsx-eslint/eslint-plugin-react
- https://github.com/facebook/react/tree/main/packages/eslint-plugin-react-hooks
- https://github.com/ArnaudBarre/eslint-plugin-react-refresh
- https://github.com/michaelfaith/eslint-plugin-package-json
- https://github.com/eslint-community/eslint-plugin-n
- https://typescript-eslint.io/rules/
- https://github.com/gajus/eslint-plugin-jsdoc
- https://github.com/schoero/eslint-plugin-better-tailwindcss
- https://github.com/CodelyTV/eslint-plugin-hexagonal-architecture
- https://github.com/btford/write-good
- https://github.com/eslint-functional/eslint-plugin-functional?tab=readme-ov-file
- https://github.com/import-js/eslint-plugin-import
- https://makenowjust-labs.github.io/recheck/docs/usage/as-eslint-plugin/
- https://github.com/ota-meshi/eslint-plugin-regexp
- https://github.com/nickdeis/eslint-plugin-no-secrets
- https://github.com/mozilla/eslint-plugin-no-unsanitized
- https://github.com/eslint-community/eslint-plugin-security
- https://github.com/Rantanen/eslint-plugin-xss
- https://github.com/lydell/eslint-plugin-simple-import-sort
- https://github.com/mskelton/eslint-plugin-playwright
- https://github.com/sindresorhus/globals
- https://github.com/azat-io/eslint-plugin-de-morgan
- https://github.com/aryelu/eslint-plugin-code-complete
- https://github.com/xojs/eslint-config-xo
