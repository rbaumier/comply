# comply TS/JS Rules — Full Inventory

Every rule from the `coding-standards` skill, sorted by how implementable it is.
Tier 1 = done. Tier 2 = next. Tier 6 = out of scope / architectural.

**Status legend:**
- ✅ done in comply
- 🟢 done via embedded oxlint config
- 🔨 to implement
- 🔬 partially mechanizable
- 🤖 LLM / review-only
- 🚫 out of scope

**Backend legend:**
- `tree-sitter` — in-process Rust AST walk via tree-sitter grammar
- `oxlint` — delegate to oxlint: comply enables the oxlint rule and remaps the diagnostic rule-id + message
- `clippy` — (v2) delegate to clippy, same remap pattern
- `tsc` — (v1.2) shell out to `tsc --noEmit`, filter diagnostic codes
- `text` — plain text / regex / filesystem check, no AST

---

## Tier 1 — Already shipping

### Custom tree-sitter rules (comply)

| ID | Rule | Backend | Source |
|----|------|---------|--------|
| ✅ R001 | `max-file-lines` — max 200 lines per file | text | File Structure |
| ✅ R002 | `max-function-lines` — max 30 lines per function | tree-sitter | Functions |
| ✅ R003 | `no-throw` — Result for all errors, never throw | tree-sitter | Error Handling |
| ✅ R004 | `no-nested-ternary` — no nested ternaries | tree-sitter | Readability |
| ✅ R005 | `banned-identifiers` — no `process`/`handle`/`data`/`do`/`execute`/`run`/`perform` | tree-sitter | Naming |

### Delegated to oxlint via bundled config

| ID | Rule | oxlint rule |
|----|------|-------------|
| 🟢 R006 | No `any` type | `typescript/no-explicit-any` |
| 🟢 R007 | No unsafe type assertion | `typescript/no-unsafe-type-assertion` |
| 🟢 R008 | Consistent type imports | `typescript/consistent-type-imports` |
| 🟢 R009 | No non-null assertion (`!`) | `typescript/no-non-null-assertion` |
| 🟢 R010 | Prefer `as const` | `typescript/prefer-as-const` |
| 🟢 R011 | Prefer `@ts-expect-error` | `typescript/prefer-ts-expect-error` |
| 🟢 R012 | No unsafe `Function` type | `typescript/no-unsafe-function-type` |
| 🟢 R013 | No `require()` imports | `typescript/no-require-imports` |
| 🟢 R014 | No default exports | `import/no-default-export` |
| 🟢 R015 | No `else` after `return` | `no-else-return` |
| 🟢 R016 | Max 2 indent levels | `max-depth` |
| 🟢 R017 | Max 3 positional args | `max-params` |
| 🟢 R018 | No magic numbers | `no-magic-numbers` |
| 🟢 R019 | Prefer `const` over `let` | `prefer-const` |
| 🟢 R020 | No `var` | `no-var` |
| 🟢 R021 | No useless `catch` | `no-useless-catch` |
| 🟢 R022 | Require curly braces | `curly` |
| 🟢 R023 | `===` over `==` | `eqeqeq` |
| 🟢 R024 | kebab-case filenames | `unicorn/filename-case` |
| 🟢 R025 | No `Array.forEach` | `unicorn/no-array-for-each` |
| 🟢 R026 | Prefer `flatMap` | `unicorn/prefer-array-flat-map` |
| 🟢 R027 | No accumulating spread | `oxc/no-accumulating-spread` |
| 🟢 R028 | No barrel files | `oxc/no-barrel-file` |
| 🟢 R029 | No misrefactored assign-op | `oxc/misrefactored-assign-op` |
| 🟢 R030 | Promise: catch-or-return | `promise/catch-or-return` |
| 🟢 R031 | Promise: always return | `promise/always-return` |
| 🟢 R032 | Promise: no multiple resolved | `promise/no-multiple-resolved` |
| 🟢 R033 | Promise: no nesting | `promise/no-nesting` |
| 🟢 R034 | Promise: no return wrap | `promise/no-return-wrap` |
| 🟢 R035 | Promise: prefer await over then | `promise/prefer-await-to-then` |
| 🟢 R036 | Promise: prefer await over callbacks | `promise/prefer-await-to-callbacks` |
| 🟢 R037 | Promise: no return in finally | `promise/no-return-in-finally` |
| 🟢 R038 | Promise: param names | `promise/param-names` |

---

## Tier 2 — High-value rules to implement next

Classified by backend. When multiple would work, pick the leftmost available.

### Naming

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R039 | `boolean-naming` — `is`/`has`/`should`/`can` prefix | **tree-sitter** | Too opinionated for oxlint. Walk var/param decls with `boolean` type annotation |
| 🔨 R040 | `explicit-units` — `delayMs`, `fileSizeKb` suffix | **tree-sitter** | Flag bare `delay`/`timeout`/`size`/`length` typed `number` |
| 🔨 R041 | `no-abbreviated-names` — `user` not `u`, `account` not `acct` | **tree-sitter** | Dictionary of banned abbreviations |
| 🔨 R042 | `no-single-letter-names` outside tight loops | **oxlint** | `id-length` with exception for iterators |
| 🔨 R043 | `no-generic-names` — `data`, `info`, `temp`, `result` | **tree-sitter** | Extend `banned-identifiers` BANNED_PREFIXES |
| 🔨 R044 | `no-type-encoded-names` — `strName`, `arrItems` | **tree-sitter** | Regex on identifier prefix |
| 🔨 R045 | `symmetric-pairs` — `getFoo`/`setFoo`, `addX`/`removeX` | **tree-sitter** | Cross-reference exports |

### Control Flow

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R046 | `no-boolean-flag-param` — `f(x, isUrgent)` → split | **tree-sitter** | Detect boolean param controlling branch |
| 🔨 R047 | `law-of-demeter` — max 1 dot deep | **tree-sitter** | Walk member_expression chains, flag depth > 1 |
| 🔨 R048 | `no-sequential-await` — `for (x of items) await f(x)` | **oxlint** | `no-await-in-loop` is close enough |
| 🔨 R049 | `prefer-switch` — 4+ `if/else if` on same discriminant | **tree-sitter** | Heuristic |
| 🔨 R050 | `no-input-mutation` — `f(arr) { arr.push(x) }` | **oxlint** | `no-param-reassign` + `no-param-properties` |

### Functions

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R051 | `no-default-params` — `function f(x = 5)` | **tree-sitter** | Too opinionated for oxlint |
| 🔨 R052 | `max-positional-args` — max 3, then options object | **oxlint** | `max-params` already enabled |
| 🔨 R053 | `pure-by-default` — no top-level mutable state reference | **tree-sitter** | Track top-level `let` + inner references |

### Data & Types

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R054 | `no-nullish-default-on-input` — `x ?? 0` on params | **tree-sitter** | Param-rooted `??`/`\|\|` detection |
| 🔨 R055 | `extract-literals` — same literal 3+ times | **tree-sitter** | Cross-reference AST |
| 🔨 R056 | `no-mutable-exports` — `export let foo` | **oxlint** | `import/no-mutable-exports` |

### Error Handling

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R057 | `timeout-on-io` — bare `await fetch()` / `await db.query()` | **tree-sitter** | No oxlint equivalent; custom IO callee detection |
| 🔨 R058 | `preserve-cause-on-rethrow` — `throw new X({ cause: e })` | **oxlint** | `unicorn/error-message` helps; extend if needed |
| 🔨 R059 | `no-empty-catch` | **oxlint** | `no-empty` with `allowEmptyCatch: false` |

### Readability

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R060 | `no-clever-coercion` — no `!!value`, `+[]`, `~~x` | **oxlint** | `no-implicit-coercion` |
| 🔨 R061 | `intermediate-variables` — 2+ ops inside arg/return | **tree-sitter** | Count operator depth in arguments |
| 🔨 R062 | `no-multi-op-oneliner` — 4+ chained ops on one line | **tree-sitter** | Heuristic |

### File Structure

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R063 | `exports-at-top` — all exports before first non-export | **tree-sitter** | AST order check |
| 🔨 R064 | `no-common-grab-bag` — `common.ts`, `shared.ts`, `utils.ts` | **text** | Filename regex at discovery |
| 🔨 R065 | `colocated-tests` — `foo.ts` needs `foo.test.ts` nearby | **text** | Filesystem check |

### Comments / Documentation

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔨 R066 | `no-commented-out-code` — heuristic detection | **text** | Regex: comment lines with `=`, `;`, `(`, `{` |
| 🔨 R067 | `todo-needs-issue-link` — `TODO` without URL / `#123` | **text** | Simple regex |
| 🔨 R068 | `jsdoc-on-exported` — every `export fn` needs `/** */` | **tree-sitter** | Check preceding comment node |
| 🔨 R069 | `module-header` — first node must be JSDoc with "What/How" | **tree-sitter** | First non-comment child check |
| 🔨 R070 | `no-restate-name-comment` — comment paraphrasing fn name | **tree-sitter** | n-gram check vs fn name |

**Tier 2 total: 32 rules — 24 tree-sitter, 6 oxlint, 2 text**

---

## Tier 3 — Needs type info (tsc / ts-morph pipeline)

Requires a TypeScript type-aware pass. Plan: new `comply typecheck` subcommand that shells out to `tsc --noEmit --pretty false --strict` and filters diagnostics by error code.

| ID | Rule | Backend | Approach |
|----|------|---------|----------|
| 🔬 R071 | `strict-typing` — no inferred `any` | **tsc** | Filter codes 7005, 7006, 7031, 7034 |
| 🔬 R072 | `option-vs-result` — `findUser` → `Option<User>` | **tsc** | Signature heuristic on `find*`/`get*` verbs |
| 🔬 R073 | `misleading-name` — `userList: Set<User>` | **tsc** | Name suffix vs declared type |
| 🔬 R074 | `data-clumps` — same 3+ fields in 2+ types | **tsc** | Cross-file structural match |
| 🔬 R075 | `boundary-condition` — unchecked `arr[0]` / `arr.length - 1` | **tsc** | `noUncheckedIndexedAccess` off → emit |
| 🔬 R076 | `no-raw-db-entity-in-handler` — handler returning Prisma entity | **tsc** | Match against `@prisma/client` types |
| 🔬 R077 | `structured-api-error` — errors need `{type,code,status,detail}` | **tsc** | Shape match |
| 🔬 R078 | `api-first` — handler without zod/openapi schema alongside | **text** | Filesystem cross-reference (no types needed) |

**Tier 3 total: 8 rules — 7 tsc, 1 text**

---

## Tier 4 — Heuristic / partial detection

| ID | Rule | Backend | Notes |
|----|------|---------|-------|
| 🔬 R079 | `one-abstraction-level` — mixing high-level + raw regex | **tree-sitter** | Regex literal + domain call in same fn |
| 🔬 R080 | `justify-inaction` — empty catch/else without comment | **tree-sitter** | Empty block + missing preceding comment |
| 🔬 R081 | `no-logger-in-business-logic` — `logger.info` in service/ | **tree-sitter** | Path-aware: flag in `services/`, `domain/`, `core/` |
| 🔬 R082 | `no-secret-in-code` — API keys, tokens | **text** | Secret-scanner regex pack |
| 🔬 R083 | `auth-on-mutation` — create/update/delete handler needs auth helper | **tree-sitter** | Cross-ref call graph |
| 🔬 R084 | `no-pii-in-logs` — `log(email, password, ssn)` | **tree-sitter** | Argument name heuristic |
| 🔬 R085 | `blank-line-between-blocks` — setup/validate/transform/return | **text** | Whitespace check (formatting) |
| 🔬 R086 | `error-message-is-remediation` — error strings need a verb | **text** | Sentence heuristic on `new Error(...)` |
| 🔬 R087 | `no-hidden-control-flow` — 3+ decorators stacked | **tree-sitter** | Count decorator nodes per function |
| 🔬 R088 | `factory-di-shape` — `create*` fns should take deps object | **tree-sitter** | AST shape on `create*` exports |

**Tier 4 total: 10 rules — 6 tree-sitter, 4 text**

---

## Tier 5 — LLM / review-only

Not a classical linter rule — requires semantic understanding.

| ID | Rule | Source |
|----|------|--------|
| 🤖 R089 | Comments answer "what goes wrong if I delete this?" | Comments |
| 🤖 R090 | Chain cause → effect across calls | Comments |
| 🤖 R091 | Explain project terms on first use | Comments |
| 🤖 R092 | Structs describe role not fields | Comments |
| 🤖 R093 | State transitions narrate journey | Comments |
| 🤖 R094 | Explain limits, invariants, boundaries | Comments |
| 🤖 R095 | Gotcha warnings + links | Comments |
| 🤖 R096 | Conversational tone | Comments |
| 🤖 R097 | Concrete numbers/names in comments | Comments |
| 🤖 R098 | Comments as sentences | Comments |
| 🤖 R099 | Emphatic word at end | Comments |
| 🤖 R100 | Intent over implementation (beyond banned prefixes) | Naming |
| 🤖 R101 | Parse, don't validate | Philosophy |
| 🤖 R102 | Make invalid states unrepresentable | Philosophy |
| 🤖 R103 | Functional core, imperative shell | Philosophy |
| 🤖 R104 | Pull complexity downward | Error Handling |
| 🤖 R105 | Define errors out of existence | Error Handling |
| 🤖 R106 | Barricade pattern | Error Handling |
| 🤖 R107 | Document impossible states | Error Handling |
| 🤖 R108 | Bound every input (reject at boundary) | Data |
| 🤖 R109 | Crosscutting via wrapping (`withTracing`) | Architecture |
| 🤖 R110 | Map DB entities to DTOs | Architecture |
| 🤖 R111 | Error messages as step-by-step remediation | Project Hygiene |

---

## Tier 6 — Architectural / cross-project

| ID | Rule | Source |
|----|------|--------|
| 🚫 R112 | Reuse before creating | Philosophy |
| 🚫 R113 | Rule of Three | Philosophy |
| 🚫 R114 | Prefer boring technology | Philosophy |
| 🚫 R115 | DRY (repo-wide) | Philosophy |
| 🚫 R116 | Vertical slices | Architecture |
| 🚫 R117 | Temporal decomposition red flag | Architecture |
| 🚫 R118 | Shotgun Surgery | Architecture |
| 🚫 R119 | Divergent Change | Architecture |
| 🚫 R120 | Module depth over shallow wrappers | Architecture |
| 🚫 R121 | No pass-through methods | Architecture |
| 🚫 R122 | Information leakage | Architecture |
| 🚫 R123 | SRP per function/module | Functions |
| 🚫 R124 | CQS — command OR query | Functions |
| 🚫 R125 | Composition over inheritance | Functions |
| 🚫 R126 | Tests/linting/CI/CD from day 1 | Project Hygiene |
| 🚫 R127 | Constrain first, relax later | Project Hygiene |
| 🚫 R128 | Codebase homogeneity | Project Hygiene |
| 🚫 R129 | Structural guardrails over discipline | Project Hygiene |
| 🚫 R130 | Hard cutover on migrations | Project Hygiene |
| 🚫 R131 | Pin all versions | Project Hygiene |
| 🚫 R132 | Group tests by feature, not type | File Structure |

---

## Architecture — `src/rules/` layout

### Principle

Each rule is a **concept** (stable `id`, remediation message, severity) with one or more **backends** — one per language. A backend can be:

- `tree-sitter` — in-process Rust AST walk
- `oxlint` — delegation to an oxlint rule, with diagnostic rule-id + message remapping
- `clippy` — (v2) delegation to a clippy lint, same remap pattern
- `tsc` — (v1.2) shell out to `tsc --noEmit`, filter by diagnostic code
- `text` — plain text / regex / filesystem check

### Folder layout

```
src/rules/
├── mod.rs                    # registry: all_rules() collects every rule module
├── walker.rs                 # shared iterative tree-sitter walker
├── backend.rs                # Backend enum + AstCheck / TextCheck traits
├── meta.rs                   # RuleMeta struct
│
├── max_file_lines/
│   ├── mod.rs                # META + register() wiring backends to languages
│   └── text.rs               # Same impl for TS/JS/TSX/Rust — plain line count
│
├── max_function_lines/
│   ├── mod.rs
│   ├── typescript.rs         # grammar tree-sitter-typescript
│   ├── tsx.rs                # grammar tree-sitter-typescript (TSX variant)
│   └── rust.rs               # (v2) grammar tree-sitter-rust
│
├── no_throw/
│   ├── mod.rs
│   └── typescript.rs         # tree-sitter throw_statement
│                             # rust.rs deferred — clippy delegation planned
│
├── no_clever_coercion/       # NEW (Tier 2)
│   └── mod.rs                # oxlint-only: no per-language impl file needed
│
├── no_default_params/        # NEW (Tier 2)
│   ├── mod.rs
│   └── typescript.rs         # tree-sitter default_value detection
│
├── boolean_naming/           # NEW (Tier 2)
│   ├── mod.rs
│   └── typescript.rs         # tree-sitter
│
├── todo_needs_issue_link/    # NEW (Tier 2)
│   ├── mod.rs
│   └── text.rs               # regex-based, same impl for every language
```

**Naming convention:**
- One folder per rule concept.
- One file per distinct **backend implementation**, named after the dominant language:
  - `typescript.rs` covers `Language::TypeScript | JavaScript` (same grammar).
  - `tsx.rs` covers `Language::Tsx` (separate JSX-aware grammar).
  - `rust.rs` covers `Language::Rust`.
  - `text.rs` for backend-agnostic text checks (line count, regex).
- If a rule doesn't apply to a language, no file for that language (ex: `no_nested_ternary/rust.rs` doesn't exist — Rust has no ternary).
- For pure `oxlint` backends, no per-language file — the binding lives in `mod.rs`.

### Key types

```rust
// src/rules/meta.rs
pub struct RuleMeta {
    pub id: &'static str,             // e.g. "no-default-params"
    pub description: &'static str,    // 1-line summary
    pub remediation: &'static str,    // full message the user sees
    pub severity: Severity,
    pub doc_url: Option<&'static str>,
}

// src/rules/backend.rs
pub enum Backend {
    TreeSitter(Box<dyn AstCheck>),
    Text(Box<dyn TextCheck>),
    Oxlint { rule: &'static str },
    Clippy { lint: &'static str },
    Tsc { diagnostic_codes: &'static [u32] },
}

pub trait AstCheck: Send + Sync {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic>;
}

pub trait TextCheck: Send + Sync {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic>;
}

// src/rules/mod.rs
pub struct Rule {
    pub meta: RuleMeta,
    pub backends: Vec<(Language, Backend)>,
}
```

### The big win: oxlintrc.json becomes generated

`src/oxlintrc.json` disappears. At startup, comply iterates over every registered rule, collects `Backend::Oxlint { rule }` entries, and writes a fresh config to the temp file. The comply rule id + remediation message are retained via a `HashMap<oxlint_rule, comply_rule_id>` remap applied in `oxlint::into_diagnostic`.

Pareil pour clippy (v2): `clippy.toml` is generated from `Backend::Clippy` registrations.

### Migration strategy

1. **Add the new types** (`meta.rs`, `backend.rs`) without touching existing rules.
2. **Introduce a `Rule` wrapper** that can hold both the old `Box<dyn Rule>` trait and the new `Rule` struct during transition.
3. **Migrate one rule at a time** to the new shape (start with `max_file_lines` — the simplest).
4. **Once all 5 are migrated**, delete the old trait.
5. **Then implement Tier 2** rules using only the new shape.

This keeps tests green at every step and avoids a big-bang refactor.

---

## Summary counts

| Tier | Count | Backend mix |
|------|-------|-------------|
| 1 custom | 5 | 4 tree-sitter + 1 text |
| 1 oxlint | 33 | all oxlint (via bundled config) |
| 2 — v1.1 backlog | 32 | 24 tree-sitter + 6 oxlint + 2 text |
| 3 — needs types | 8 | 7 tsc + 1 text |
| 4 — heuristic | 10 | 6 tree-sitter + 4 text |
| 5 — LLM | 23 | n/a |
| 6 — architectural | 21 | n/a |
| **Total** | **132** | |

**After Tier 2 shipping**: 70 rules mechanically enforced (5+33 already + 32 new).

---

## v1.1 Priority Order

Top 10 by bang-for-buck:

1. **R039 `boolean-naming`** (tree-sitter) — catches every decl
2. **R067 `todo-needs-issue-link`** (text) — one regex, real hygiene value
3. **R051 `no-default-params`** (tree-sitter) — one AST check, hidden coupling
4. **R068 `jsdoc-on-exported`** (tree-sitter) — forces documentation culture
5. **R063 `exports-at-top`** (tree-sitter) — trivial AST order check
6. **R057 `timeout-on-io`** (tree-sitter) — real correctness bug
7. **R047 `law-of-demeter`** (tree-sitter) — catches real coupling
8. **R048 `no-sequential-await`** (oxlint) — flip a config flag
9. **R046 `no-boolean-flag-param`** (tree-sitter) — catches real design smell
10. **R066 `no-commented-out-code`** (text) — heuristic but high-signal
