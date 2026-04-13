# comply — Remaining Rules

Rules not yet implemented. The catalog (`comply catalog --json`) is the source of truth for what ships today.

---

## Tier 0 — Performance debt

### Native clone-detection rule (replaces jscpd)

**Status:** jscpd subprocess is DISABLED in `src/main.rs` (`lint_typescript`, `lint_rust`). `src/jscpd.rs` is kept compiled under `#![allow(dead_code)]` as a reference implementation to port from.

**Why it was disabled:** the perf audit showed jscpd was responsible for **92% of wall-clock on a 216-file run (~105ms/file)**. It respawns a Node.js runtime on every call, parses its own JSON report, and does not amortize anything across invocations. Wall-clock breakdown from `tools/bench-baseline.json` before the disable:

| target | total | jscpd share |
|---|---|---|
| tiny-ts (1 TS file) | 1.48s | **936ms (63%)** |
| small-rules (2 RS files) | 1.23s | 223ms (18%) |
| many-rules (216 RS files) | **24.58s** | **22.64s (92%)** |

After the disable, many-rules dropped to **1.73s** (−93%) and the dominant phase became `engine (rs)` at 88%. jscpd is the single biggest liability in comply's runtime.

**What needs building:** an in-process, native Rust clone-detection rule with the same semantics as the old `no-clones` rule_id. Minimum requirements:

- **Token-based hashing.** Tokenize each file using the existing tree-sitter grammars (we already parse TS/TSX/JS/Rust in `src/engine.rs`). Strip identifiers/literals so renamed clones still match. Emit a stream of "normalized token kinds" per file.
- **Rolling N-gram fingerprint.** Slide a window of N tokens (start with N=50, the old jscpd `min-tokens` threshold) and compute a hash per window. Store `(hash, file_path, start_row, end_row)` entries in a `HashMap<u64, Vec<Location>>`.
- **Report duplicates.** Any hash with ≥2 locations becomes one diagnostic on the first location, with a message naming the partner location(s). Merge overlapping windows on the same file pair into a single span.
- **Cross-file, per-run scope.** The rule runs once at the engine level with visibility into ALL files of the batch (not per-file like the current TreeSitter/Text backends). This means adding a new `Backend::WholeBatch` variant — or a dedicated post-pass phase — so the rule sees the whole corpus.
- **Language buckets.** Only compare within the same language family (don't match a TS token stream against a Rust one).
- **Config knobs.** Expose `min_tokens` and `ignore` globs via `comply.toml` under `[rules.no-clones]`, with defaults mirroring the old jscpd config.
- **Suppression.** Respect `// comply-ignore-next-line no-clones` via the existing `ignore_comments` mechanism.

**Target performance.** On 216 files the native version should run in ≤100ms (current engine overall is 1.5s for all other rules combined; clone detection should be a fraction of that). Well over 100× faster than jscpd.

**Reference:** read `src/jscpd.rs` for the legacy wiring (directory grouping, diagnostic shape, rule_id). Delete that file once the native rule ships.

**Acceptance:**
- New rule `no-clones` fires on a minimal integration test fixture with two near-identical functions and on a negative fixture with only coincidental similarities.
- `tools/bench.ts` shows the new rule contributing <10% of the `engine (rs)` phase on `many-rules`.
- Re-enable the rule everywhere the old jscpd block used to run (see deleted code in `lint_typescript` / `lint_rust`).

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

### All plugins evaluated & implemented

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
- ✅ eslint-plugin-jsx-a11y (33 rules)
- ✅ eslint-plugin-package-json (2 rules)
- ✅ eslint-plugin-better-tailwindcss (2 rules)
- ✅ eslint-plugin-hexagonal-architecture (1 rule)
- ✅ write-good (1 rule)
- ✅ eslint-plugin-no-secrets (patterns added to no-hardcoded-secret)
- ✅ eslint-plugin-xss (patterns added to no-dynamic-template)
- ✅ eslint-plugin-security (4 rules)
- ✅ eslint-plugin-no-unsanitized (patterns added)
- ✅ eslint-plugin-jsdoc (7 rules)
- ✅ eslint-plugin-playwright (10 rules)
- ✅ eslint-plugin-de-morgan (1 rule)
- ✅ eslint-plugin-simple-import-sort (covered by existing rules)
- ✅ eslint-plugin-jsx-a11y (33 rules — separate commit)

---

- https://philodev.one/posts/2026-04-code-complexity/
- hono : https://www.evlog.dev/
- improve perf 
- cargo run -- src
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
     Running `target/debug/comply src`

thread 'main' (61010216) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
byte index 11 is not a char boundary; it is inside '…' (bytes 10..13) of `* .filter(…).shift() — always flag *`
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---

mode human: dans les diagnostiques afficher avec une flèche qui pointe et affiche le code (il doit y avoir une lib pour ça)

---
