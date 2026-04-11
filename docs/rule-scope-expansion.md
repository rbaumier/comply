# Rule Scope Expansion TODO

> Audit date: 2026-04-11
> Total native rules: 540 directories + delegated rules
> Languages supported: TypeScript, TSX, JavaScript, Rust, Vue

---

## 1. Add Rust backend (general patterns via TreeSitter)

These rules detect universal programming patterns. They currently use `register_ts_family!` or `TS_FAMILY` with TreeSitter backends. Adding Rust requires writing a `rust::Check` module using Rust tree-sitter AST nodes.

**Priority: HIGH** -- these are the highest-value expansions (real bugs/smells in any language).

| Rule | Current | Rust equivalent |
|------|---------|----------------|
| `cognitive-complexity` | TS+JS+TSX | `if_expression`, `match_expression`, `for_expression`, `while_expression`, `loop_expression`, nested closures |
| `cyclomatic-complexity` | TS+JS+TSX | Same concept -- `if`, `match` arms, `while`, `for`, `loop`, `&&`/`\|\|` |
| `expression-complexity` | TS+JS+TSX | Same concept -- deeply nested boolean/arithmetic expressions |
| `nested-control-flow` | TS+JS+TSX | Same concept -- nested `if`/`match`/`for`/`while`/`loop` |
| `no-nested-switch` | TS+JS+TSX | Nested `match` expressions |
| `no-identical-expressions` | TS+JS+TSX | `a == a`, `a && a`, `a \|\| a` -- same concept |
| `no-identical-conditions` | TS+JS+TSX | Same `if`/`else if` condition repeated |
| `no-identical-functions` | TS+JS+TSX | Duplicate `fn` bodies |
| `no-duplicated-branches` | TS+JS+TSX | Same block in different `if`/`match` arms |
| `no-all-duplicated-branches` | TS+JS+TSX | Every branch has identical body |
| `no-redundant-boolean` | TS+JS+TSX | `if x { true } else { false }` -- just return `x` |
| `no-redundant-jump` | TS+JS+TSX | Unnecessary `continue`/`break`/`return` at end of block |
| `no-collapsible-if` | TS+JS+TSX | `if a { if b { ... } }` -> `if a && b { ... }` |
| `no-lonely-if` | TS+JS+TSX | `else { if ... }` -> `else if ...` |
| `no-negated-condition` | TS+JS+TSX | `if !x { a } else { b }` -> swap branches |
| `no-inverted-boolean-check` | TS+JS+TSX | `!(a == b)` -> `a != b` |
| `de-morgan-simplify` | TS+JS+TSX | `!(a && b)` -> `!a \|\| !b` |
| `no-inconsistent-returns` | TS+JS+TSX | Some paths return a value, others don't (Rust enforces this, but early `return;` vs `return val` in `-> ()` fns) |
| `no-invariant-returns` | TS+JS+TSX | All branches return the same value |
| `no-gratuitous-expression` | TS+JS+TSX | Condition always true/false based on previous checks |
| `no-nested-assignment` | TS+JS+TSX | `let x = y = 5;` patterns (less common in Rust but possible with shadowing) |
| `no-empty-collection-use` | TS+JS+TSX | Using a collection right after creating it empty |
| `no-unused-collection` | TS+JS+TSX | Collection populated but never read |
| `no-element-overwrite` | TS+JS+TSX | Consecutive writes to same index/key |
| `no-useless-increment` | TS+JS+TSX | `x += 1; return x - 1` patterns (less common in Rust) |
| `no-redundant-assignment` | TS+JS+TSX | Assigning to a variable that's immediately overwritten |
| `no-double-cast` | TS+JS+TSX | `x as Foo as Bar` -- double `as` casts |
| `prefer-immediate-return` | TS+JS+TSX | `let x = expr; return x;` -> `return expr;` |
| `too-many-break-or-continue` | TS+JS+TSX | Excessive `break`/`continue` in loops |
| `prefer-while` | TS+JS+TSX | `loop { if !cond { break; } ... }` -> `while cond { ... }` |
| `no-loop-counter-reassign` | TS+JS+TSX | Reassigning loop counter inside loop body |
| `no-misplaced-loop-counter` | TS+JS+TSX | Counter increment in wrong place |
| `for-loop-increment-sign` | TS+JS+TSX | For loop with wrong increment direction (less common in Rust's `for in` but possible with manual counters) |
| `arguments-order` | TS+JS+TSX | Swapped argument names suggesting wrong order |
| `data-clumps` | TS+JS+TSX | Same group of params repeated across fns -> extract struct |
| `operation-returning-nan` | TS+JS+TSX | `f64::NAN` operations in Rust -- e.g., `0.0 / 0.0` |

**Estimated effort**: Each rule needs a new `rust.rs` module with tree-sitter queries against Rust AST node types. Rules sharing patterns (e.g., all the "identical/duplicated" family) can share helpers via `rust_helpers.rs`.

---

## 2. TextCheck rules: add missing Rust coverage

These use `TS_FAMILY` constant (`[TypeScript, Tsx, JavaScript]`) with `Backend::Text`. Since text checks scan raw source with regex/string matching (no AST), adding Rust is trivial: just include `Language::Rust` in the backends array.

**Priority: HIGH** -- zero implementation effort, just registration change.

### 2a. Security/secrets (should definitely cover Rust)

| Rule | Current | Should add | Rationale |
|------|---------|------------|-----------|
| `no-hardcoded-ip` | TS+JS+TSX | Rust | Scans string literals for IP addresses -- same in Rust |
| `no-clear-text-protocol` | TS+JS+TSX | Rust | Scans for `http://`, `ftp://` in strings |
| `no-hardcoded-secret-signature` | TS+JS+TSX | Rust | Scans for known secret patterns (AWS keys, etc.) |

### 2b. Code quality (universal patterns)

| Rule | Current | Should add | Rationale |
|------|---------|------------|-----------|
| `no-duplicate-string` | TS+JS+TSX | Rust | Duplicate string literals across file -- same concept |
| `no-empty-file` | TS+JS+TSX | Rust | Empty source file detection |
| `comment-prose-quality` | TS+JS+TSX | Rust | Comment quality checks on `//` and `///` comments |
| `expiring-todo-comments` | TS+JS+TSX | Rust | `// TODO(2026-01-01)` expiry -- `//` comments same in Rust |
| `colocated-tests` | TS+JS+TSX | Rust | Check test file co-location patterns (if applicable to Rust projects) |
| `justify-inaction` | TS+JS+TSX | Rust | Comments explaining why something isn't done |

### 2c. SQL text rules (Rust uses sqlx/diesel/sea-orm with SQL strings)

| Rule | Current | Should add | Rationale |
|------|---------|------------|-----------|
| `sql-no-between-timestamp` | TS+JS+TSX | Rust | `sqlx::query!("SELECT ... BETWEEN ...")` |
| `sql-no-float-for-money` | TS+JS+TSX | Rust | `FLOAT`/`DOUBLE` in SQL strings |
| `sql-no-like-wildcard-prefix` | TS+JS+TSX | Rust | `LIKE '%foo'` prevents index use |
| `sql-no-offset-pagination` | TS+JS+TSX | Rust | `OFFSET` in SQL strings |
| `sql-no-pg-enum` | TS+JS+TSX | Rust | PostgreSQL ENUM type in migrations |
| `sql-no-select-star` | TS+JS+TSX | Rust | `SELECT *` in SQL strings |
| `sql-no-timestamp-without-tz` | TS+JS+TSX | Rust | `TIMESTAMP` without `WITH TIME ZONE` |
| `sql-no-varchar` | TS+JS+TSX | Rust | `VARCHAR` -> prefer `TEXT` |
| `sql-prefer-exists-over-in` | TS+JS+TSX | Rust | `WHERE x IN (SELECT ...)` -> `EXISTS` |

### 2d. Tailwind rules (add Vue)

| Rule | Current | Should add | Rationale |
|------|---------|------------|-----------|
| `tailwind-no-conflicting-classes` | TS+JS+TSX | Vue | Vue templates use Tailwind too |
| `tailwind-no-duplicate-classes` | TS+JS+TSX | Vue | Same |

---

## 3. Missing JavaScript coverage (TS+TSX but not JS)

| Rule | Current | Should add | Rationale |
|------|---------|------------|-----------|
| `no-misleading-collection-name` | TS+TSX | JavaScript | JS has the same `Set`/`Map` types, no reason to exclude |

---

## 4. Regex text rules: add Rust coverage (conditional)

All 20+ `regex_*` rules use `TS_FAMILY` with text backend. They extract patterns from JavaScript regex literals (`/pattern/flags`). Rust uses `Regex::new("pattern")` instead.

**These cannot trivially expand to Rust** -- the extraction function `extract_regex_patterns()` looks for `/pattern/` syntax. To support Rust, each would need a second extractor for `Regex::new("...")` / `regex!("...")` patterns.

| Rules (batch) | Current | Effort to add Rust |
|---------------|---------|-------------------|
| `regex-anchor-precedence`, `regex-complexity`, `regex-no-control-chars`, `regex-no-duplicate-chars`, `regex-no-empty-after-reluctant`, `regex-no-empty-alternative`, `regex-no-empty-character-class`, `regex-no-empty-group`, `regex-no-empty-lookaround`, `regex-no-empty-string-match`, `regex-no-escape-backspace`, `regex-no-invisible-character`, `regex-no-misleading-char-class`, `regex-no-multiple-spaces`, `regex-no-obscure-range`, `regex-no-octal`, `regex-no-single-char-class`, `regex-no-slow-pattern`, `regex-no-standalone-backslash`, `regex-no-stateful-global`, `regex-no-unused-groups`, `regex-no-useless-lazy`, `regex-no-useless-two-nums-quantifier`, `regex-no-zero-quantifier`, `regex-prefer-char-class`, `regex-prefer-quantifier`, `regex-sort-flags`, `regex-use-unicode-flag` | TS+JS+TSX | Medium -- need shared `extract_rust_regex_pattern()` helper, then all rules benefit. ~28 rules unlocked with 1 shared extractor. |

---

## 5. Infrastructure: Vue SFC `<script>` extraction

Vue SFC files are currently text-only (`Backend::Text`). The `Language::Vue` enum variant exists but the comment in `files.rs` says: *"text-based rules only (no tree-sitter grammar bundled)"*.

If a tree-sitter Vue grammar or `<script>` extraction were added, it would unlock the entire TS/JS rule catalog for Vue files (~300+ rules). Currently only 4 Vue-specific text rules exist:
- `vue-no-duplicate-v-if`
- `vue-no-options-api`
- `vue-no-reactive-destructure`
- `vue-v-for-needs-stable-key`

**Impact**: High (300+ rules), **Effort**: Medium (extract `<script>` block, parse as TS/JS).

---

## 6. Already maximal coverage

These rules already cover all applicable languages -- no expansion needed.

### Text rules with Rust coverage
| Rule | Languages |
|------|-----------|
| `banned-comment-words` | TS+JS+TSX+Rust |
| `max-file-lines` | TS+JS+TSX+Rust |
| `no-bidi-characters` | TS+JS+TSX+Rust+Vue |
| `no-commented-out-code` | TS+JS+TSX+Rust |
| `no-common-grab-bag` | TS+JS+TSX+Rust |
| `no-hardcoded-secret` | TS+JS+TSX+Rust |
| `no-section-divider-comments` | TS+JS+TSX+Rust |
| `todo-needs-issue-link` | TS+JS+TSX+Rust |

### TS+JS+TSX rules with Rust (native or clippy delegation)
| Rule | Rust binding |
|------|-------------|
| `banned-identifiers` | clippy::disallowed_names |
| `boolean-naming` | native Rust backend |
| `explicit-units` | native Rust backend |
| `exports-at-top` | native Rust backend (`pub` at top) |
| `jsdoc-on-exported` | missing_docs |
| `max-function-lines` | clippy::too_many_lines |
| `module-header` | missing_docs |
| `no-abbreviated-names` | native Rust backend |
| `no-boolean-flag-param` | clippy::fn_params_excessive_bools |
| `no-generic-names` | clippy::disallowed_names |
| `no-multi-op-oneliner` | native Rust backend |
| `no-new-regex-with-variable` | native Rust backend |
| `no-skipped-test-without-link` | native Rust backend |
| `no-throw` | clippy::panic |
| `no-type-encoded-names` | native Rust backend |
| `no-verb-in-rest-url` | native Rust backend |
| `prefer-switch-over-chained-if` | clippy::comparison_chain |
| `timeout-on-io` | native Rust backend |

### Rust-only rules (28 rules)
All `rust_*` prefixed rules -- inherently Rust-specific, no expansion needed.

### Domain-specific rules (no expansion makes sense)
- **a11y_*** (34 rules) -- JSX/HTML accessibility, not applicable to Rust
- **react_*** (20 rules) -- React-specific
- **hono_*** (8 rules) -- Hono.js framework
- **playwright_*** (10 rules) -- Playwright test framework
- **node_*** (7 rules) -- Node.js APIs
- **ts_*** (14 rules) -- TypeScript type system
- **vue_*** (4 rules) -- Vue.js framework
- **jsdoc_*** (10 rules) -- JSDoc comments
- **drizzle_*** (2 rules) -- Drizzle ORM
- **tanstack_*** (2 rules) -- TanStack Query
- **zod_*** (2 rules) -- Zod schemas
- **package_json_*** (2 rules) -- package.json
- **tailwind-no-dynamic-class** -- JSX className expressions

---

## Summary

| Category | Rules affected | Effort |
|----------|---------------|--------|
| TextCheck: add Rust (security+quality+SQL) | 18 rules | **Trivial** -- add `Language::Rust` to backends array |
| TextCheck: add Vue to tailwind rules | 2 rules | **Trivial** -- add `Language::Vue` |
| Missing JS in TS+TSX rule | 1 rule | **Trivial** -- add JS backend |
| Regex rules: Rust extractor | 28 rules | **Medium** -- 1 shared extractor unlocks all |
| TreeSitter: Rust backend (general patterns) | 36 rules | **High** -- each needs `rust.rs` module |
| Vue SFC `<script>` extraction | ~300+ rules unlocked | **Medium** -- infrastructure change |
