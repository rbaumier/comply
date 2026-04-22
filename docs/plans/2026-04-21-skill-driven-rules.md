# Skill-driven rules — Implementation Plan

> **Status: COMPLETE — 2026-04-22.** All 82 rules implemented, registered, tests green (4364 total). Checkboxes flipped mechanically.

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-development (recommended) or plans skill to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement ~82 new native comply rules derived from the skill library, covering 15 technology domains that currently have zero or inadequate static analysis.

**Architecture:** Each rule follows the existing pattern: one `src/rules/{snake_case}/` directory, `mod.rs` (RuleMeta + register), `typescript.rs` or `text.rs` backend, inline tests. No new infra. See CLAUDE.md for the full skeleton.

**Tech Stack:** Rust, tree-sitter TypeScript/Rust/Vue grammars, cargo nextest, `src/rules/test_helpers.rs`

---

## Reference: Rule skeleton

Every rule below maps to this structure. Only the detection logic differs.

```
src/rules/{snake_case}/
  mod.rs         ← RuleMeta + register()
  typescript.rs  ← crate::ast_check! { |node, source, ctx, diagnostics| ... }
  text.rs        ← impl TextCheck (only for text-inherent rules)
```

`mod.rs` boilerplate:
```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rule-id",
    description: "One-line.",
    remediation: "Actionable fix.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["category"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

Registration in `src/rules/mod.rs`:
1. `pub mod rule_name;` in the module list
2. `rule_name::register()` in `all_rule_defs()`

Test commands:
```bash
cargo nextest run rule_name        # scoped, fast
cargo nextest run                  # full suite after each batch
cargo clippy --all --all-targets -- -D warnings
```

---

## Batch 1 — TypeScript / Architecture

**Rules:** `no-default-export`, `prefer-promise-all`, `ts-prefer-using-declaration`
**Deferred:** `ts-prefer-satisfies` (requires type-checker), `no-conditional-async-return` (data-flow)

### Task 1.1 — `no-default-export`

**Files:**
- Create: `src/rules/no_default_export/mod.rs`
- Create: `src/rules/no_default_export/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Add mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "no-default-export",
    description: "Default exports break tree-shaking and refactoring.",
    remediation: "Use a named export: `export function foo()` instead of `export default function foo()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["typescript"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **Step 2: Write failing tests in typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // placeholder — will fail compile until implemented
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_default_function() {
        assert_eq!(run("export default function foo() {}").len(), 1);
    }

    #[test]
    fn flags_default_class() {
        assert_eq!(run("export default class Foo {}").len(), 1);
    }

    #[test]
    fn flags_default_expression() {
        assert_eq!(run("const x = 1; export default x;").len(), 1);
    }

    #[test]
    fn allows_named_export() {
        assert!(run("export function foo() {}").is_empty());
    }

    #[test]
    fn allows_named_class_export() {
        assert!(run("export class Foo {}").is_empty());
    }

    #[test]
    fn allows_re_export_default() {
        // Re-exporting default from another module is not a declaration
        assert!(run("export { default } from './foo';").is_empty());
    }
}
```

- [x] **Step 3: Run tests — expect compile error or failures**
```bash
cargo nextest run no_default_export 2>&1 | head -30
```

- [x] **Step 4: Implement detection in typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "export_statement" { return; }
    let text = &source[node.byte_range()];
    // `export default X` but not `export { X as default }` or `export { default } from`
    if !text.starts_with("export default ") && !text.starts_with("export default\n") {
        return;
    }
    // Re-exports: "export default from" is a syntax error; "export { default }" starts with "export {"
    // so the above check is sufficient.
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Default exports are forbidden. Use a named export instead.".into(),
        severity: Severity::Warning,
    });
}
```

- [x] **Step 5: Register the rule** — add to `src/rules/mod.rs`:
  - `pub mod no_default_export;` in the pub mod block
  - `no_default_export::register()` in `all_rule_defs()`

- [x] **Step 6: Run tests**
```bash
cargo nextest run no_default_export
# Expected: all tests pass
```

- [x] **Step 7: Full suite + clippy**
```bash
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
```

- [x] **Step 8: Commit**
```bash
git add src/rules/no_default_export/ src/rules/mod.rs
git commit -m "feat(no-default-export): flag export default declarations"
```

---

### Task 1.2 — `prefer-promise-all`

**Files:**
- Create: `src/rules/prefer_promise_all/mod.rs`
- Create: `src/rules/prefer_promise_all/typescript.rs`
- Modify: `src/rules/mod.rs`

**Detection logic:** Find `lexical_declaration` nodes (`const`/`let`) whose initializer is an `await_expression`. When two or more such declarations appear consecutively in the same `statement_block`, AND none of the awaited call expressions reference identifiers bound by the others, flag each as a sequential await that could be parallelised.

Conservative simplification: flag any two consecutive `const X = await callExpr()` statements where the second `callExpr` is a plain call (not `callExpr(X)` — i.e., does not contain the text of the first binding name).

- [x] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_two_independent_awaits() {
        let src = r#"
async function f() {
  const a = await fetchUser();
  const b = await fetchPosts();
}
"#;
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn allows_dependent_await() {
        // b depends on a — sequential is correct
        let src = r#"
async function f() {
  const a = await fetchUser();
  const b = await fetchPosts(a.id);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_await() {
        assert!(run("async function f() { const a = await fetch(); }").is_empty());
    }

    #[test]
    fn allows_promise_all_already() {
        let src = r#"
async function f() {
  const [a, b] = await Promise.all([fetchUser(), fetchPosts()]);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_three_independent_awaits() {
        let src = r#"
async function f() {
  const a = await fetchA();
  const b = await fetchB();
  const c = await fetchC();
}
"#;
        // All three flagged
        assert!(run(src).len() >= 2);
    }
}
```

- [x] **Step 2: Implement detection**

Detection algorithm (in `ast_check!`):
1. On `statement_block` nodes, collect child statements that match `lexical_declaration` with a single `await_expression` initializer.
2. Find consecutive runs of such statements (no intervening non-await statements).
3. For each run of 2+: check each awaited call's source text against all binding names in the run. If a call's text contains a binding name from an earlier statement → the call is dependent, break the run at that point.
4. Flag each statement in independent sub-runs of 2+ with a diagnostic.

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "statement_block" { return; }

    // Collect consecutive await-assignment statements
    struct AwaitStmt {
        binding: String,  // the bound name (left-hand side)
        call_text: String, // the awaited expression text
        row: usize,
        col: usize,
    }

    let mut run: Vec<AwaitStmt> = Vec::new();

    let flush = |run: &mut Vec<AwaitStmt>, diagnostics: &mut Vec<Diagnostic>, ctx: &_| {
        if run.len() >= 2 {
            for stmt in run.iter() {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: stmt.row + 1,
                    column: stmt.col + 1,
                    rule_id: super::META.id.into(),
                    message: "Sequential awaits on independent calls: use Promise.all() instead.".into(),
                    severity: Severity::Warning,
                });
            }
        }
        run.clear();
    };

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "lexical_declaration" {
            flush(&mut run, diagnostics, ctx);
            continue;
        }
        // Must have single declarator with await initializer
        let decl = match child.named_child(0) {
            Some(d) if d.kind() == "variable_declarator" => d,
            _ => { flush(&mut run, diagnostics, ctx); continue; }
        };
        let name_node = match decl.child_by_field_name("name") {
            Some(n) => n,
            None => { flush(&mut run, diagnostics, ctx); continue; }
        };
        let val_node = match decl.child_by_field_name("value") {
            Some(v) => v,
            None => { flush(&mut run, diagnostics, ctx); continue; }
        };
        if val_node.kind() != "await_expression" {
            flush(&mut run, diagnostics, ctx);
            continue;
        }
        let binding = source[name_node.byte_range()].to_owned();
        let call_text = source[val_node.byte_range()].to_owned();
        let pos = child.start_position();

        // Check if this call depends on any binding in the current run
        let dependent = run.iter().any(|s| call_text.contains(&s.binding));
        if dependent {
            flush(&mut run, diagnostics, ctx);
        }
        run.push(AwaitStmt { binding, call_text, row: pos.row, col: pos.column });
    }
    flush(&mut run, diagnostics, ctx);
}
```

- [x] **Step 3: Register, run tests, clippy, commit**
```bash
cargo nextest run prefer_promise_all
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/prefer_promise_all/ src/rules/mod.rs
git commit -m "feat(prefer-promise-all): flag sequential independent awaits"
```

---

### Task 1.3 — `ts-prefer-using-declaration`

**Files:**
- Create: `src/rules/ts_prefer_using_declaration/mod.rs`
- Create: `src/rules/ts_prefer_using_declaration/typescript.rs`
- Modify: `src/rules/mod.rs`

**Detection logic:** Flag `try_statement` where the `finally_clause` contains only a single `expression_statement` calling `.close()`, `.dispose()`, `.destroy()`, `.disconnect()`, or `.release()` on a variable — a pattern replaceable by `await using` / `using`.

- [x] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    fn run(s: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_try_finally_close() {
        assert_eq!(run("const c = connect(); try { use(c) } finally { c.close() }").len(), 1);
    }

    #[test]
    fn flags_try_finally_dispose() {
        assert_eq!(run("const r = open(); try { r.read() } finally { r.dispose() }").len(), 1);
    }

    #[test]
    fn flags_try_finally_disconnect() {
        assert_eq!(run("try { query(db) } finally { db.disconnect() }").len(), 1);
    }

    #[test]
    fn allows_finally_with_multiple_statements() {
        // Not just a cleanup — keep it
        assert!(run("try { f() } finally { cleanup(); log() }").is_empty());
    }

    #[test]
    fn allows_finally_with_error_handling() {
        assert!(run("try { f() } finally { if (err) throw err }").is_empty());
    }
}
```

- [x] **Step 2: Implement detection**

```rust
use crate::diagnostic::{Diagnostic, Severity};

const CLEANUP_METHODS: &[&str] = &["close", "dispose", "destroy", "disconnect", "release", "end"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "try_statement" { return; }
    let finally = match node.child_by_field_name("finalizer") {
        Some(f) => f,
        None => return,
    };
    // finally body must have exactly one named child (the statement)
    let named: Vec<_> = finally.named_children(&mut finally.walk()).collect();
    if named.len() != 1 { return; }
    let stmt = named[0];
    if stmt.kind() != "expression_statement" { return; }
    let expr = match stmt.named_child(0) {
        Some(e) => e,
        None => return,
    };
    if expr.kind() != "call_expression" { return; }
    let func = match expr.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.kind() != "member_expression" { return; }
    let prop = match func.child_by_field_name("property") {
        Some(p) => p,
        None => return,
    };
    let method = &source[prop.byte_range()];
    if !CLEANUP_METHODS.contains(&method) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Use `using` / `await using` instead of try/finally with `.{method}()` (TS 5.2+)."
        ),
        severity: Severity::Warning,
    });
}
```

- [x] **Step 3: Register, run tests, clippy, commit**
```bash
cargo nextest run ts_prefer_using_declaration
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/ts_prefer_using_declaration/ src/rules/mod.rs
git commit -m "feat(ts-prefer-using-declaration): flag try/finally cleanup replaceable by using"
```

---

## Batch 2 — React

**Rules (7):** `react-server-action-requires-validation`, `react-server-action-requires-auth`, `react-prefer-use-transition`, `react-no-inline-default-prop`, `react-passive-event-listeners`, `react-no-derived-state-in-effect`, `react-use-state-initializer-function`

---

### Task 2.1 — `react-server-action-requires-validation`

**Files:**
- Create: `src/rules/react_server_action_requires_validation/mod.rs`
- Create: `src/rules/react_server_action_requires_validation/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-server-action-requires-validation",
    description: "Server Actions with parameters must validate input before use.",
    remediation: "Add `schema.parse(input)` or `schema.safeParse(input)` at the top of the Server Action body.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.lines().take(5).any(|l| {
            let t = l.trim();
            t == "'use server'" || t == r#""use server""#
        }) {
            return vec![];
        }
        if src.contains(".parse(") || src.contains(".safeParse(") || src.contains(".input(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if !t.starts_with("export async function") { continue; }
            if let Some(open) = t.find('(') {
                let after = &t[open + 1..];
                if let Some(close) = after.find(')') {
                    if close > 0 {
                        diags.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: i + 1,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: "Server Action with parameters must validate input with `.parse()` or `.safeParse()`.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("actions.ts"), src))
    }

    #[test]
    fn flags_params_no_parse() {
        assert_eq!(run("'use server'\nexport async function del(id: string) { await db.delete(x) }").len(), 1);
    }

    #[test]
    fn allows_with_parse() {
        assert!(run("'use server'\nexport async function del(input: unknown) { schema.parse(input); }").is_empty());
    }

    #[test]
    fn allows_no_params() {
        assert!(run("'use server'\nexport async function list() { return db.select() }").is_empty());
    }

    #[test]
    fn allows_non_server_file() {
        assert!(run("export async function del(id: string) { await db.delete(x) }").is_empty());
    }
}
```

- [x] **Step 3: Register** — `src/rules/mod.rs`: `pub mod react_server_action_requires_validation;` + `react_server_action_requires_validation::register()`

- [x] **Step 4: Test & commit**
```bash
cargo nextest run react_server_action_requires_validation
git add src/rules/react_server_action_requires_validation/ src/rules/mod.rs
git commit -m "feat(react-server-action-requires-validation): flag unvalidated server action params"
```

---

### Task 2.2 — `react-server-action-requires-auth`

**Files:**
- Create: `src/rules/react_server_action_requires_auth/mod.rs`
- Create: `src/rules/react_server_action_requires_auth/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-server-action-requires-auth",
    description: "Server Actions with mutations must check authentication.",
    remediation: "Call `getSession()` or `auth()` and verify the result before performing mutations.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.lines().take(5).any(|l| {
            let t = l.trim();
            t == "'use server'" || t == r#""use server""#
        }) {
            return vec![];
        }
        if !src.contains(".insert(") && !src.contains(".update(") && !src.contains(".delete(") {
            return vec![];
        }
        if src.contains("getSession(") || src.contains("auth()") || src.contains("verifySession")
            || src.contains("requireAuth") || src.contains("currentUser(")
        {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("export async function") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Server Action with mutations must verify authentication before proceeding.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("actions.ts"), src))
    }

    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(run("'use server'\nexport async function create(t: string) { await db.insert(posts).values({ t }) }").len(), 1);
    }

    #[test]
    fn allows_with_get_session() {
        assert!(run("'use server'\nexport async function create(t: string) { const s = await getSession(); await db.insert(posts).values({ t }) }").is_empty());
    }

    #[test]
    fn allows_read_only() {
        assert!(run("'use server'\nexport async function list() { return db.select().from(posts) }").is_empty());
    }

    #[test]
    fn allows_non_server_file() {
        assert!(run("export async function create(t: string) { await db.insert(posts).values({ t }) }").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run react_server_action_requires_auth
git add src/rules/react_server_action_requires_auth/ src/rules/mod.rs
git commit -m "feat(react-server-action-requires-auth): flag unauthenticated server action mutations"
```

---

### Task 2.3 — `react-prefer-use-transition`

**Files:**
- Create: `src/rules/react_prefer_use_transition/mod.rs`
- Create: `src/rules/react_prefer_use_transition/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-prefer-use-transition",
    description: "Replace manual `loading` state with `useTransition` for concurrent-safe async UI.",
    remediation: "Replace `const [loading, setLoading] = useState(false)` + manual setLoading calls with `const [isPending, startTransition] = useTransition()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if src.contains("useTransition") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if !t.contains("useState(false)") || !t.contains("const [") { continue; }
            // Extract setter: const [_, setter] = useState(false)
            if let Some(comma_pos) = t.find(", ") {
                let after = &t[comma_pos + 2..];
                if let Some(bracket) = after.find(']') {
                    let setter = after[..bracket].trim();
                    if !setter.is_empty()
                        && src.contains(&format!("{setter}(true)"))
                        && src.contains(&format!("{setter}(false)"))
                        && src.contains("await ")
                    {
                        diags.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: i + 1,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "Replace manual `{setter}(true/false)` loading state with `useTransition`."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_manual_loading_state() {
        let src = "const [loading, setLoading] = useState(false)\nasync function submit() { setLoading(true); await post(); setLoading(false) }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_use_transition() {
        let src = "const [isPending, startTransition] = useTransition()\nconst [loading, setLoading] = useState(false)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_await() {
        let src = "const [loading, setLoading] = useState(false)\nfunction submit() { setLoading(true); post(); setLoading(false) }";
        assert!(run(src).is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run react_prefer_use_transition
git add src/rules/react_prefer_use_transition/ src/rules/mod.rs
git commit -m "feat(react-prefer-use-transition): flag manual loading boolean state"
```

---

### Task 2.4 — `react-no-inline-default-prop`

**Files:**
- Create: `src/rules/react_no_inline_default_prop/mod.rs`
- Create: `src/rules/react_no_inline_default_prop/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-inline-default-prop",
    description: "Non-primitive default props in `memo()` create new references every render, breaking memoization.",
    remediation: "Define the default value outside the component: `const EMPTY: T[] = []` then `{ items = EMPTY }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **Step 2: Create typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "memo" && func_text != "React.memo" { return; }

    let call_text = node.utf8_text(source).unwrap_or("");
    // Params appear before the first => in the call
    if let Some(arrow_pos) = call_text.find("=>") {
        let params = &call_text[..arrow_pos];
        if params.contains("= []") || params.contains("= {}")
            || params.contains("= () =>") || params.contains("= new ")
        {
            let pos = func.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: "Non-primitive default prop inside `memo()` creates a new reference every render. Move it outside the component.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_array_default() {
        assert_eq!(run("const C = memo(({ items = [] }) => <div />)").len(), 1);
    }

    #[test]
    fn flags_object_default() {
        assert_eq!(run("const C = memo(({ style = {} }) => <div />)").len(), 1);
    }

    #[test]
    fn flags_fn_default() {
        assert_eq!(run("const C = memo(({ onClick = () => {} }) => <div />)").len(), 1);
    }

    #[test]
    fn allows_primitive_default() {
        assert!(run("const C = memo(({ count = 0 }) => <span>{count}</span>)").is_empty());
    }

    #[test]
    fn allows_identifier_default() {
        assert!(run("const NOOP = () => {}; const C = memo(({ onClick = NOOP }) => <div />)").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run react_no_inline_default_prop
git add src/rules/react_no_inline_default_prop/ src/rules/mod.rs
git commit -m "feat(react-no-inline-default-prop): flag non-primitive defaults in memo()"
```

---

### Task 2.5 — `react-passive-event-listeners`

**Files:**
- Create: `src/rules/react_passive_event_listeners/mod.rs`
- Create: `src/rules/react_passive_event_listeners/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-passive-event-listeners",
    description: "Scroll/touch/wheel listeners should be passive to avoid blocking the main thread.",
    remediation: "Pass `{ passive: true }` as the third argument: `addEventListener('wheel', handler, { passive: true })`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **Step 2: Create typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

const SCROLL_EVENTS: &[&str] = &["touchstart", "touchmove", "wheel", "scroll"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.kind() != "member_expression" { return; }
    let prop = match func.child_by_field_name("property") {
        Some(p) => p,
        None => return,
    };
    if prop.utf8_text(source).unwrap_or("") != "addEventListener" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let event_arg = match args.named_child(0) {
        Some(a) => a,
        None => return,
    };
    let event_text = event_arg.utf8_text(source).unwrap_or("");
    let event_name = event_text.trim_matches(|c| c == '\'' || c == '"');
    if !SCROLL_EVENTS.contains(&event_name) { return; }

    let has_passive = match args.named_child(2) {
        Some(opt) => {
            let t = opt.utf8_text(source).unwrap_or("");
            t.contains("passive: true") || t.contains("passive:true")
        }
        None => false,
    };
    if !has_passive {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Add `{{ passive: true }}` to `addEventListener('{event_name}', ...)` to avoid jank."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_touchstart_no_options() {
        assert_eq!(run("window.addEventListener('touchstart', handler)").len(), 1);
    }

    #[test]
    fn flags_wheel_no_passive() {
        assert_eq!(run("el.addEventListener('wheel', handler, { capture: true })").len(), 1);
    }

    #[test]
    fn allows_passive_true() {
        assert!(run("window.addEventListener('touchstart', handler, { passive: true })").is_empty());
    }

    #[test]
    fn allows_click_no_passive() {
        assert!(run("btn.addEventListener('click', handler)").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run react_passive_event_listeners
git add src/rules/react_passive_event_listeners/ src/rules/mod.rs
git commit -m "feat(react-passive-event-listeners): flag scroll/touch listeners without passive:true"
```

---

### Task 2.6 — `react-no-derived-state-in-effect`

**Files:**
- Create: `src/rules/react_no_derived_state_in_effect/mod.rs`
- Create: `src/rules/react_no_derived_state_in_effect/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-derived-state-in-effect",
    description: "`useEffect` whose body only calls a state setter derives state — move the derivation to render.",
    remediation: "Replace `useEffect(() => { setX(a + b) }, [a, b])` with `const x = a + b` computed directly during render.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **Step 2: Create typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.utf8_text(source).unwrap_or("") != "useEffect" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let callback = match args.named_child(0) {
        Some(c) => c,
        None => return,
    };
    if callback.kind() != "arrow_function" { return; }

    let body = match callback.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };
    if body.kind() != "statement_block" { return; }

    let stmts: Vec<_> = body.named_children(&mut body.walk()).collect();
    if stmts.len() != 1 { return; }

    let stmt = stmts[0];
    if stmt.kind() != "expression_statement" { return; }
    let expr = match stmt.named_child(0) {
        Some(e) => e,
        None => return,
    };
    if expr.kind() != "call_expression" { return; }

    let call_text = expr.utf8_text(source).unwrap_or("");
    if call_text.contains("await") || call_text.contains("fetch(")
        || call_text.contains("subscribe(") || call_text.contains("addEventListener(")
    {
        return;
    }

    let inner_func = match expr.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if !inner_func.utf8_text(source).unwrap_or("").starts_with("set") { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Derived state in `useEffect` is an anti-pattern. Compute the value during render instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_setter_only_effect() {
        assert_eq!(run("useEffect(() => { setFull(first + ' ' + last) }, [first, last])").len(), 1);
    }

    #[test]
    fn allows_effect_with_fetch() {
        assert!(run("useEffect(() => { fetch('/api').then(setData) }, [id])").is_empty());
    }

    #[test]
    fn allows_multi_statement_effect() {
        assert!(run("useEffect(() => { const x = a + b; setFull(x); log(x) }, [a, b])").is_empty());
    }

    #[test]
    fn allows_non_setter_call() {
        assert!(run("useEffect(() => { cleanup() }, [])").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run react_no_derived_state_in_effect
git add src/rules/react_no_derived_state_in_effect/ src/rules/mod.rs
git commit -m "feat(react-no-derived-state-in-effect): flag setter-only useEffect bodies"
```

---

### Task 2.7 — `react-use-state-initializer-function`

**Files:**
- Create: `src/rules/react_use_state_initializer_function/mod.rs`
- Create: `src/rules/react_use_state_initializer_function/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-use-state-initializer-function",
    description: "Expensive `useState` initial values should use a lazy initializer `() => expr`.",
    remediation: "Replace `useState(expensiveCall())` with `useState(() => expensiveCall())` so the computation only runs once.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **Step 2: Create typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

const EXPENSIVE_PREFIXES: &[&str] = &[
    "localStorage.", "sessionStorage.", "JSON.parse(", "compute", "build", "create", "parse(",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.utf8_text(source).unwrap_or("") != "useState" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let init = match args.named_child(0) {
        Some(i) => i,
        None => return,
    };
    // Skip primitives and lazy init (arrow function) and plain identifiers
    match init.kind() {
        "number" | "string" | "true" | "false" | "null" | "undefined"
        | "arrow_function" | "identifier" => return,
        "call_expression" => {}
        _ => return,
    }
    let init_text = init.utf8_text(source).unwrap_or("");
    if EXPENSIVE_PREFIXES.iter().any(|p| init_text.starts_with(p)) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Pass a lazy initializer `() => expr` to `useState` to avoid recomputing on every render.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_local_storage() {
        assert_eq!(run("useState(localStorage.getItem('x'))").len(), 1);
    }

    #[test]
    fn flags_json_parse() {
        assert_eq!(run("useState(JSON.parse(raw))").len(), 1);
    }

    #[test]
    fn allows_lazy_init() {
        assert!(run("useState(() => localStorage.getItem('x'))").is_empty());
    }

    #[test]
    fn allows_primitive() {
        assert!(run("useState(0)").is_empty());
        assert!(run("useState(false)").is_empty());
        assert!(run("useState(null)").is_empty());
    }

    #[test]
    fn allows_identifier() {
        assert!(run("useState(initialValue)").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run react_use_state_initializer_function
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/react_use_state_initializer_function/ src/rules/mod.rs
git commit -m "feat(react-use-state-initializer-function): flag expensive useState initial values"
```

---

## Batch 3 — Tailwind

**Rules (5):** `tailwind-no-important-modifier`, `tailwind-no-arbitrary-z-index`, `tailwind-prefer-size-shorthand`, `tailwind-no-apply-for-variants`, `tailwind-prefer-cn-utility`

---

### Task 3.1 — `tailwind-no-important-modifier`

**Files:**
- Create: `src/rules/tailwind_no_important_modifier/mod.rs`
- Create: `src/rules/tailwind_no_important_modifier/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-important-modifier",
    description: "The Tailwind `!` important modifier signals a specificity fight, not a real fix.",
    remediation: "Fix the specificity issue instead of using `!` — restructure class order or use a more specific selector.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("className") && !line.contains("class=") { continue; }
            if let Some(col) = find_important_class(line) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Avoid the Tailwind `!` important modifier — fix specificity instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

fn find_important_class(line: &str) -> Option<usize> {
    let start = line.find("className").or_else(|| line.find("class="))?;
    let after = &line[start..];
    let bytes = after.as_bytes();
    for i in 0..bytes.len().saturating_sub(1) {
        if bytes[i] == b'!' && bytes[i + 1].is_ascii_lowercase() {
            return Some(start + i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_important_class() {
        assert_eq!(run(r#"<div className="!text-red-500 flex" />"#).len(), 1);
    }

    #[test]
    fn flags_important_in_middle() {
        assert_eq!(run(r#"<div className="w-full !hidden" />"#).len(), 1);
    }

    #[test]
    fn allows_normal_classes() {
        assert!(run(r#"<div className="text-red-500 flex" />"#).is_empty());
    }

    #[test]
    fn allows_exclamation_outside_classname() {
        assert!(run(r#"<input placeholder="!important note" />"#).is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tailwind_no_important_modifier
git add src/rules/tailwind_no_important_modifier/ src/rules/mod.rs
git commit -m "feat(tailwind-no-important-modifier): flag ! important modifier in className"
```

---

### Task 3.2 — `tailwind-no-arbitrary-z-index`

**Files:**
- Create: `src/rules/tailwind_no_arbitrary_z_index/mod.rs`
- Create: `src/rules/tailwind_no_arbitrary_z_index/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-arbitrary-z-index",
    description: "Arbitrary z-index values `z-[n]` bypass the design token scale.",
    remediation: "Use a design token (`z-10`, `z-50`) or define a custom token in `tailwind.config.ts`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("className") && !line.contains("class=") { continue; }
            if let Some(pos) = line.find("z-[") {
                let after = &line[pos + 3..];
                if after.starts_with(|c: char| c.is_ascii_digit()) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: pos + 1,
                        rule_id: super::META.id.into(),
                        message: "Use a design token (e.g. `z-10`, `z-50`) instead of an arbitrary z-index value.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_arbitrary_z() {
        assert_eq!(run(r#"<div className="z-[100] relative" />"#).len(), 1);
    }

    #[test]
    fn flags_large_z() {
        assert_eq!(run(r#"<div className="z-[9999]" />"#).len(), 1);
    }

    #[test]
    fn allows_token_z() {
        assert!(run(r#"<div className="z-10 relative" />"#).is_empty());
    }

    #[test]
    fn allows_named_z() {
        assert!(run(r#"<div className="z-modal" />"#).is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tailwind_no_arbitrary_z_index
git add src/rules/tailwind_no_arbitrary_z_index/ src/rules/mod.rs
git commit -m "feat(tailwind-no-arbitrary-z-index): flag z-[n] arbitrary z-index values"
```

---

### Task 3.3 — `tailwind-prefer-size-shorthand`

**Files:**
- Create: `src/rules/tailwind_prefer_size_shorthand/mod.rs`
- Create: `src/rules/tailwind_prefer_size_shorthand/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-size-shorthand",
    description: "`w-X h-X` with equal values can be written as `size-X`.",
    remediation: "Replace `w-4 h-4` with `size-4` (Tailwind v3.4+).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("className") && !line.contains("class=") { continue; }
            if let Some(val) = find_wh_duplicate(line) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Replace `w-{val} h-{val}` with `size-{val}`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

fn find_wh_duplicate(line: &str) -> Option<String> {
    let w_vals: Vec<&str> = line
        .split_whitespace()
        .filter_map(|t| {
            // strip trailing " ' > characters common in JSX
            let t = t.trim_matches(|c| c == '"' || c == '\'' || c == '>');
            t.strip_prefix("w-")
        })
        .collect();
    let h_vals: Vec<&str> = line
        .split_whitespace()
        .filter_map(|t| {
            let t = t.trim_matches(|c| c == '"' || c == '\'' || c == '>');
            t.strip_prefix("h-")
        })
        .collect();
    for w in &w_vals {
        if h_vals.contains(w) {
            return Some(w.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_equal_w_h() {
        assert_eq!(run(r#"<div className="w-4 h-4 flex" />"#).len(), 1);
    }

    #[test]
    fn flags_full() {
        assert_eq!(run(r#"<div className="w-full h-full" />"#).len(), 1);
    }

    #[test]
    fn allows_different_values() {
        assert!(run(r#"<div className="w-4 h-6" />"#).is_empty());
    }

    #[test]
    fn allows_size_shorthand_already() {
        assert!(run(r#"<div className="size-4" />"#).is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tailwind_prefer_size_shorthand
git add src/rules/tailwind_prefer_size_shorthand/ src/rules/mod.rs
git commit -m "feat(tailwind-prefer-size-shorthand): suggest size-X for matching w-X h-X"
```

---

### Task 3.4 — `tailwind-no-apply-for-variants`

**Files:**
- Create: `src/rules/tailwind_no_apply_for_variants/mod.rs`
- Create: `src/rules/tailwind_no_apply_for_variants/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-apply-for-variants",
    description: "`@apply` outside `@layer base` defeats Tailwind's purging and specificity model.",
    remediation: "Compose classes in JSX/HTML instead, or use CSS variables for theming. Reserve `@apply` for `@layer base` resets only.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    // Register for all languages — filter to .css files in the check body.
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let ext = ctx.path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "css" { return vec![]; }

        let mut diags = Vec::new();
        let mut in_base_layer = false;
        let mut brace_depth: usize = 0;

        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("@layer base") || t.starts_with("@layer typography") {
                in_base_layer = true;
            }
            brace_depth += t.chars().filter(|&c| c == '{').count();
            let closing = t.chars().filter(|&c| c == '}').count();
            if closing >= brace_depth {
                brace_depth = 0;
                in_base_layer = false;
            } else {
                brace_depth -= closing;
            }
            if t.starts_with("@apply") && !in_base_layer {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Avoid `@apply` outside `@layer base` — compose classes in JSX or use CSS variables.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_css(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("styles.css"), src))
    }

    fn run_ts(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_apply_in_component() {
        assert_eq!(run_css(".btn { @apply px-4 py-2 rounded; }").len(), 1);
    }

    #[test]
    fn allows_apply_in_base_layer() {
        assert!(run_css("@layer base {\n  body { @apply font-sans; }\n}").is_empty());
    }

    #[test]
    fn ignores_non_css_files() {
        assert!(run_ts("@apply px-4").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tailwind_no_apply_for_variants
git add src/rules/tailwind_no_apply_for_variants/ src/rules/mod.rs
git commit -m "feat(tailwind-no-apply-for-variants): flag @apply outside @layer base"
```

---

### Task 3.5 — `tailwind-prefer-cn-utility`

**Files:**
- Create: `src/rules/tailwind_prefer_cn_utility/mod.rs`
- Create: `src/rules/tailwind_prefer_cn_utility/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-prefer-cn-utility",
    description: "Ternary or concatenation in `className` should use `cn()` or `clsx()` for readability.",
    remediation: "Replace `className={x ? 'a' : 'b'}` with `className={cn('a', { b: x })}`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **Step 2: Create typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" { return; }
    let name = match node.child_by_field_name("name") {
        Some(n) => n,
        None => return,
    };
    if name.utf8_text(source).unwrap_or("") != "className" { return; }

    let val = match node.child_by_field_name("value") {
        Some(v) => v,
        None => return,
    };
    if val.kind() != "jsx_expression" { return; }

    let val_text = val.utf8_text(source).unwrap_or("");
    if val_text.contains("cn(") || val_text.contains("clsx(") || val_text.contains("cva(") {
        return;
    }

    let inner = match val.named_child(0) {
        Some(i) => i,
        None => return,
    };
    if inner.kind() == "ternary_expression" || inner.kind() == "binary_expression" {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &inner,
            super::META.id,
            "Use `cn()` or `clsx()` for conditional class names instead of ternaries or concatenation.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_ternary_classname() {
        assert_eq!(run(r#"<div className={x ? 'flex' : 'hidden'} />"#).len(), 1);
    }

    #[test]
    fn allows_cn_utility() {
        assert!(run(r#"<div className={cn('p-4', x && 'flex')} />"#).is_empty());
    }

    #[test]
    fn allows_static_classname() {
        assert!(run(r#"<div className="flex p-4" />"#).is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tailwind_prefer_cn_utility
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/tailwind_prefer_cn_utility/ src/rules/mod.rs
git commit -m "feat(tailwind-prefer-cn-utility): flag ternary in className without cn()"
```

---

## Batch 4 — Database SQL

**Rules (4):** `sql-create-index-concurrently`, `sql-nullable-requires-comment`, `sql-advisory-lock-prefer-xact`, `sql-require-transaction-timeout`

All SQL rules register on all 5 languages and scan text (SQL appears in template literals in TS/Rust files).

---

### Task 4.1 — `sql-create-index-concurrently`

**Files:**
- Create: `src/rules/sql_create_index_concurrently/mod.rs`
- Create: `src/rules/sql_create_index_concurrently/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-create-index-concurrently",
    description: "`CREATE INDEX` without `CONCURRENTLY` takes an `ACCESS EXCLUSIVE` lock, blocking all table access.",
    remediation: "Use `CREATE INDEX CONCURRENTLY` for production migrations. Run outside a transaction block.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql", "migrations"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if !upper.contains("CREATE") || !upper.contains("INDEX") { continue; }
            if upper.contains("CONCURRENTLY") { continue; }
            // Match: CREATE [UNIQUE] INDEX <name> ON ... (not CONCURRENTLY)
            let trimmed = upper.trim();
            if trimmed.starts_with("CREATE INDEX") || trimmed.starts_with("CREATE UNIQUE INDEX") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`CREATE INDEX` without `CONCURRENTLY` locks the table. Use `CREATE INDEX CONCURRENTLY` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_create_index() {
        assert_eq!(run("CREATE INDEX idx_email ON users(email);").len(), 1);
    }

    #[test]
    fn flags_create_unique_index() {
        assert_eq!(run("CREATE UNIQUE INDEX idx_ref ON orders(reference);").len(), 1);
    }

    #[test]
    fn allows_concurrently() {
        assert!(run("CREATE INDEX CONCURRENTLY idx_email ON users(email);").is_empty());
    }

    #[test]
    fn flags_in_template_literal() {
        assert_eq!(run("const q = `CREATE INDEX idx_x ON t(x)`;").len(), 1);
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run sql_create_index_concurrently
git add src/rules/sql_create_index_concurrently/ src/rules/mod.rs
git commit -m "feat(sql-create-index-concurrently): flag CREATE INDEX without CONCURRENTLY"
```

---

### Task 4.2 — `sql-nullable-requires-comment`

**Files:**
- Create: `src/rules/sql_nullable_requires_comment/mod.rs`
- Create: `src/rules/sql_nullable_requires_comment/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-nullable-requires-comment",
    description: "Nullable columns must have a `--` comment explaining why NULL is allowed.",
    remediation: "Add a `-- reason: <why this can be NULL>` comment on the preceding line.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

// SQL type keywords that appear in column definitions
const SQL_TYPES: &[&str] = &[
    "INTEGER", "INT", "BIGINT", "SMALLINT", "TEXT", "VARCHAR", "CHAR",
    "BOOLEAN", "BOOL", "TIMESTAMP", "DATE", "DECIMAL", "NUMERIC", "FLOAT",
    "REAL", "DOUBLE", "UUID", "JSONB", "JSON", "BYTEA", "SERIAL", "BIGSERIAL",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let upper = line.to_ascii_uppercase();
            let t = upper.trim();
            // Column definition: starts with identifier then SQL type, no NOT NULL
            if !SQL_TYPES.iter().any(|ty| t.contains(ty)) { continue; }
            if t.contains("NOT NULL") || t.contains("PRIMARY KEY") { continue; }
            if t.starts_with("CREATE") || t.starts_with("ALTER") || t.starts_with("--") { continue; }
            // Looks like a column def — check if preceding line has a comment
            let prev_is_comment = i > 0 && lines[i - 1].trim().starts_with("--");
            let has_inline_comment = line.contains("--");
            if !prev_is_comment && !has_inline_comment {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Nullable column has no comment explaining why NULL is allowed.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("migration.ts"), src))
    }

    #[test]
    fn flags_nullable_without_comment() {
        assert_eq!(run("  deleted_at TIMESTAMP,").len(), 1);
    }

    #[test]
    fn allows_nullable_with_inline_comment() {
        assert!(run("  deleted_at TIMESTAMP, -- null until soft-deleted").is_empty());
    }

    #[test]
    fn allows_nullable_with_preceding_comment() {
        assert!(run("  -- null until user completes profile\n  avatar_url TEXT,").is_empty());
    }

    #[test]
    fn allows_not_null() {
        assert!(run("  email TEXT NOT NULL,").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run sql_nullable_requires_comment
git add src/rules/sql_nullable_requires_comment/ src/rules/mod.rs
git commit -m "feat(sql-nullable-requires-comment): flag nullable columns without rationale comment"
```

---

### Task 4.3 — `sql-advisory-lock-prefer-xact`

**Files:**
- Create: `src/rules/sql_advisory_lock_prefer_xact/mod.rs`
- Create: `src/rules/sql_advisory_lock_prefer_xact/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-advisory-lock-prefer-xact",
    description: "`pg_advisory_lock` holds until session ends, leaking if the connection is reused. Use `pg_advisory_xact_lock` instead.",
    remediation: "Replace `pg_advisory_lock(key)` with `pg_advisory_xact_lock(key)` — it auto-releases at transaction end.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(text::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            // pg_advisory_lock( is the session-scoped variant; skip xact and try_ variants
            if !line.contains("pg_advisory_lock(") { continue; }
            if line.contains("pg_advisory_xact_lock(") || line.contains("pg_try_advisory") { continue; }
            if let Some(col) = line.find("pg_advisory_lock(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Use `pg_advisory_xact_lock()` instead of `pg_advisory_lock()` — it releases automatically at transaction end.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_session_lock() {
        assert_eq!(run("SELECT pg_advisory_lock(123);").len(), 1);
    }

    #[test]
    fn allows_xact_lock() {
        assert!(run("SELECT pg_advisory_xact_lock(123);").is_empty());
    }

    #[test]
    fn allows_try_lock() {
        assert!(run("SELECT pg_try_advisory_lock(123);").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run sql_advisory_lock_prefer_xact
git add src/rules/sql_advisory_lock_prefer_xact/ src/rules/mod.rs
git commit -m "feat(sql-advisory-lock-prefer-xact): flag session-scoped advisory locks"
```

---

### Task 4.4 — `sql-require-transaction-timeout`

**Files:**
- Create: `src/rules/sql_require_transaction_timeout/mod.rs`
- Create: `src/rules/sql_require_transaction_timeout/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "sql-require-transaction-timeout",
    description: "DB connection pool config should set `statement_timeout` and `idle_in_transaction_session_timeout` to prevent runaway queries.",
    remediation: "Add `statement_timeout: '30s'` and `idle_in_transaction_session_timeout: '60s'` to the pool config.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("new Pool(") && !src.contains("drizzle(") && !src.contains("createPool(") {
            return vec![];
        }
        if src.contains("statement_timeout") { return vec![]; }

        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("new Pool(") || t.contains("= new Pool(") || t.contains("drizzle(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break; // flag once per file
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("db.ts"), src))
    }

    #[test]
    fn flags_pool_without_timeout() {
        assert_eq!(run("const pool = new Pool({ connectionString: url })").len(), 1);
    }

    #[test]
    fn allows_pool_with_timeout() {
        assert!(run("const pool = new Pool({ connectionString: url, statement_timeout: '30s' })").is_empty());
    }

    #[test]
    fn ignores_non_pool_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run sql_require_transaction_timeout
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/sql_require_transaction_timeout/ src/rules/mod.rs
git commit -m "feat(sql-require-transaction-timeout): flag pool config missing statement_timeout"
```

---

## Batch 5 — Rust

**Rules (7):** `rust-prefer-once-lock`, `rust-vec-with-capacity`, `rust-prefer-channel-over-arc-mutex-vec`, `rust-anyhow-context-on-question-mark`, `rust-must-use-on-result-fn`, `rust-unsafe-ffi-isolation`, `rust-thiserror-for-lib`

---

### Task 5.1 — `rust-prefer-once-lock`

**Files:**
- Create: `src/rules/rust_prefer_once_lock/mod.rs`
- Create: `src/rules/rust_prefer_once_lock/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-once-lock",
    description: "`lazy_static!` and `once_cell` are superseded by `std::sync::OnceLock`/`LazyLock` (Rust 1.70+).",
    remediation: "Replace `lazy_static! { static ref X: T = ... }` with `static X: LazyLock<T> = LazyLock::new(|| ...);`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            let flagged = t.starts_with("lazy_static!")
                || t.contains("once_cell::sync::Lazy")
                || t.contains("once_cell::sync::OnceCell");
            if flagged {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `std::sync::LazyLock` or `OnceLock` (stable since Rust 1.70) instead of `lazy_static!` or `once_cell`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_lazy_static_macro() {
        assert_eq!(run("lazy_static! { static ref FOO: String = String::new(); }").len(), 1);
    }

    #[test]
    fn flags_once_cell_lazy() {
        assert_eq!(run("static FOO: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| compute());").len(), 1);
    }

    #[test]
    fn allows_std_once_lock() {
        assert!(run("static FOO: std::sync::OnceLock<String> = std::sync::OnceLock::new();").is_empty());
    }

    #[test]
    fn allows_lazy_lock() {
        assert!(run("static FOO: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| compute());").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_prefer_once_lock
git add src/rules/rust_prefer_once_lock/ src/rules/mod.rs
git commit -m "feat(rust-prefer-once-lock): flag lazy_static! and once_cell in favour of std::sync"
```

---

### Task 5.2 — `rust-vec-with-capacity`

**Files:**
- Create: `src/rules/rust_vec_with_capacity/mod.rs`
- Create: `src/rules/rust_vec_with_capacity/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-vec-with-capacity",
    description: "`Vec::new()` followed by a for-loop with `.push()` reallocates repeatedly. Use `Vec::with_capacity()` when the size is known.",
    remediation: "Replace `Vec::new()` with `Vec::with_capacity(source.len())` when iterating a collection of known length.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if !t.contains("Vec::new()") || !t.starts_with("let mut ") { continue; }
            // Extract variable name: `let mut name = Vec::new()`
            let after = &t["let mut ".len()..];
            let var = match after.split_whitespace().next() {
                Some(v) => v,
                None => continue,
            };
            let push_pattern = format!("{var}.push(");
            let look_ahead = &lines[i + 1..std::cmp::min(i + 20, lines.len())];
            let has_for = look_ahead.iter().any(|l| l.contains("for ") && l.contains(" in "));
            let has_push = look_ahead.iter().any(|l| l.contains(&push_pattern));
            if has_for && has_push {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Use `Vec::with_capacity(...)` instead of `Vec::new()` when `{var}` is populated in a for-loop."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_vec_new_then_push_in_for() {
        let src = "let mut result = Vec::new();\nfor item in items {\n    result.push(item);\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_capacity() {
        let src = "let mut result = Vec::with_capacity(items.len());\nfor item in items {\n    result.push(item);\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_vec_new_no_for() {
        assert!(run("let mut v = Vec::new();\nv.push(1);").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_vec_with_capacity
git add src/rules/rust_vec_with_capacity/ src/rules/mod.rs
git commit -m "feat(rust-vec-with-capacity): flag Vec::new() before for-push loops"
```

---

### Task 5.3 — `rust-prefer-channel-over-arc-mutex-vec`

**Files:**
- Create: `src/rules/rust_prefer_channel_over_arc_mutex_vec/mod.rs`
- Create: `src/rules/rust_prefer_channel_over_arc_mutex_vec/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-prefer-channel-over-arc-mutex-vec",
    description: "`Arc<Mutex<Vec<` for collecting task results adds contention. Use `mpsc::channel` instead.",
    remediation: "Use `let (tx, rx) = mpsc::channel(); ... tx.send(result); let results: Vec<_> = rx.iter().collect();`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("Arc::new(Mutex::new(Vec") { return vec![]; }
        if !src.contains(".lock()") { return vec![]; }
        if !src.contains(".push(") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("Arc::new(Mutex::new(Vec") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `mpsc::channel` instead of `Arc<Mutex<Vec>>` to collect results from concurrent tasks.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_arc_mutex_vec_with_push() {
        let src = "let results = Arc::new(Mutex::new(Vec::new()));\nlet r = results.clone();\nthread::spawn(move || r.lock().unwrap().push(compute()));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_channel() {
        let src = "let (tx, rx) = mpsc::channel();\nthread::spawn(move || tx.send(compute()).unwrap());\nlet results: Vec<_> = rx.iter().collect();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arc_mutex_without_push() {
        assert!(run("let x = Arc::new(Mutex::new(Vec::new()));").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_prefer_channel_over_arc_mutex_vec
git add src/rules/rust_prefer_channel_over_arc_mutex_vec/ src/rules/mod.rs
git commit -m "feat(rust-prefer-channel-over-arc-mutex-vec): flag Arc<Mutex<Vec>> collect pattern"
```

---

### Task 5.4 — `rust-anyhow-context-on-question-mark`

**Files:**
- Create: `src/rules/rust_anyhow_context_on_question_mark/mod.rs`
- Create: `src/rules/rust_anyhow_context_on_question_mark/rust.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod rust;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-anyhow-context-on-question-mark",
    description: "`?` without `.context()` produces bare error messages with no callsite information.",
    remediation: "Chain `.context(\"what you were doing\")` before `?` so errors carry actionable context.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
```

- [x] **Step 2: Create rust.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "?" { return; }

    // Only application crates (not library code)
    let path_str = ctx.path.to_string_lossy();
    if !path_str.contains("main.rs") && !path_str.contains("src/bin/") && !path_str.contains("src/cli") {
        return;
    }

    // The inner expression (thing before ?)
    let inner = match node.named_child(0) {
        Some(i) => i,
        None => return,
    };
    let inner_text = inner.utf8_text(source).unwrap_or("");
    if inner_text.contains(".context(") || inner_text.contains(".with_context(") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `.context(\"description\")` before `?` to give this error actionable context.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_path(s, &Check, "src/main.rs")
    }

    fn run_lib(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_bare_question_mark_in_main() {
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x")?; Ok(s) }"#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_context_before_question_mark() {
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x").context("reading file")?; Ok(s) }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_lib_files() {
        let src = r#"fn load() -> anyhow::Result<String> { let s = std::fs::read_to_string("x")?; Ok(s) }"#;
        assert!(run_lib(src).is_empty());
    }
}
```

**Note:** `run_rust_with_path` needs to be added to `test_helpers.rs` if it doesn't exist yet. Check first:
```bash
grep -r "run_rust_with_path" src/rules/test_helpers.rs
```
If missing, add alongside `run_ts_with_path` following the same pattern (same as `run_rust` but accepts a `fake_path` param). Alternatively, skip path-dependent test and mark the test as `#[ignore]`.

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_anyhow_context_on_question_mark
git add src/rules/rust_anyhow_context_on_question_mark/ src/rules/mod.rs
git commit -m "feat(rust-anyhow-context-on-question-mark): flag bare ? in application crates"
```

---

### Task 5.5 — `rust-must-use-on-result-fn`

**Files:**
- Create: `src/rules/rust_must_use_on_result_fn/mod.rs`
- Create: `src/rules/rust_must_use_on_result_fn/rust.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod rust;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-must-use-on-result-fn",
    description: "Public functions returning `Result` should be `#[must_use]` so callers can't silently discard errors.",
    remediation: "Add `#[must_use]` above the function definition.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
```

- [x] **Step 2: Create rust.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "function_item" { return; }

    // Must be pub
    let vis = match node.child_by_field_name("visibility_modifier") {
        Some(v) => v,
        None => return,
    };
    if vis.utf8_text(source).unwrap_or("") != "pub" { return; }

    // Must return Result<
    let ret = match node.child_by_field_name("return_type") {
        Some(r) => r,
        None => return,
    };
    if !ret.utf8_text(source).unwrap_or("").contains("Result<") { return; }

    // Check for #[must_use] in the 5 lines before this function
    let pos = node.start_position();
    let lines: Vec<&str> = ctx.source.lines().collect();
    let check_from = pos.row.saturating_sub(5);
    let preceding = &lines[check_from..pos.row];
    if preceding.iter().any(|l| l.contains("#[must_use]")) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Add `#[must_use]` — public functions returning `Result` must not allow callers to silently discard errors.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_pub_result_without_must_use() {
        assert_eq!(run("pub fn connect() -> Result<String, Error> { Ok(String::new()) }").len(), 1);
    }

    #[test]
    fn allows_must_use_attribute() {
        assert!(run("#[must_use]\npub fn connect() -> Result<String, Error> { Ok(String::new()) }").is_empty());
    }

    #[test]
    fn allows_private_fn() {
        assert!(run("fn connect() -> Result<String, Error> { Ok(String::new()) }").is_empty());
    }

    #[test]
    fn allows_non_result_return() {
        assert!(run("pub fn name() -> String { String::new() }").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_must_use_on_result_fn
git add src/rules/rust_must_use_on_result_fn/ src/rules/mod.rs
git commit -m "feat(rust-must-use-on-result-fn): flag pub fn returning Result without #[must_use]"
```

---

### Task 5.6 — `rust-unsafe-ffi-isolation`

**Files:**
- Create: `src/rules/rust_unsafe_ffi_isolation/mod.rs`
- Create: `src/rules/rust_unsafe_ffi_isolation/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-unsafe-ffi-isolation",
    description: "`extern \"C\"` blocks should be isolated inside a `mod sys`, `mod ffi`, or `mod raw` module.",
    remediation: "Move the `extern \"C\"` block into a dedicated submodule: `mod sys { extern \"C\" { ... } }`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SAFE_MOD_NAMES: &[&str] = &["mod sys", "mod ffi", "mod raw", "mod bindings"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut in_safe_mod = false;
        let mut depth: usize = 0;

        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if SAFE_MOD_NAMES.iter().any(|m| t.starts_with(m)) {
                in_safe_mod = true;
            }
            depth += t.chars().filter(|&c| c == '{').count();
            let closing = t.chars().filter(|&c| c == '}').count();
            if closing >= depth {
                depth = 0;
                in_safe_mod = false;
            } else {
                depth -= closing;
            }
            if (t.starts_with("extern \"C\"") || t.starts_with("extern \"system\"")) && !in_safe_mod {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Isolate `extern \"C\"` inside `mod sys { ... }` or `mod ffi { ... }`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.rs"), src))
    }

    #[test]
    fn flags_extern_c_at_root() {
        assert_eq!(run("extern \"C\" { fn foo(); }").len(), 1);
    }

    #[test]
    fn allows_extern_c_in_sys_mod() {
        assert!(run("mod sys {\n    extern \"C\" { fn foo(); }\n}").is_empty());
    }

    #[test]
    fn allows_extern_c_in_ffi_mod() {
        assert!(run("mod ffi {\n    extern \"C\" { fn bar(); }\n}").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_unsafe_ffi_isolation
git add src/rules/rust_unsafe_ffi_isolation/ src/rules/mod.rs
git commit -m "feat(rust-unsafe-ffi-isolation): flag extern C blocks outside sys/ffi modules"
```

---

### Task 5.7 — `rust-thiserror-for-lib`

**Files:**
- Create: `src/rules/rust_thiserror_for_lib/mod.rs`
- Create: `src/rules/rust_thiserror_for_lib/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-thiserror-for-lib",
    description: "Library error types should derive `thiserror::Error` instead of manually implementing `Display`.",
    remediation: "Add `#[derive(thiserror::Error)]` and use `#[error(\"...\")]` attributes on enum variants.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Rust, Backend::Text(Box::new(text::Check)))],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Skip application entry points
        let path_str = ctx.path.to_string_lossy();
        if path_str.contains("main.rs") || path_str.contains("src/bin/") { return vec![]; }
        // If thiserror is already used in this file, skip
        if ctx.source.contains("thiserror") { return vec![]; }

        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("pub enum ") && t.contains("Error") && !t.starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `#[derive(thiserror::Error)]` for library error types — avoids boilerplate `Display` impls.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("src/error.rs"), src))
    }

    #[test]
    fn flags_pub_enum_error_without_thiserror() {
        assert_eq!(run("pub enum MyError { NotFound, Unauthorized }").len(), 1);
    }

    #[test]
    fn allows_enum_with_thiserror() {
        assert!(run("#[derive(thiserror::Error)]\npub enum MyError { #[error(\"not found\")] NotFound }").is_empty());
    }

    #[test]
    fn ignores_main_rs() {
        let ctx = crate::rules::backend::CheckCtx::for_test(
            Path::new("src/main.rs"),
            "pub enum MyError { Fail }",
        );
        assert!(Check.check(&ctx).is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run rust_thiserror_for_lib
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/rust_thiserror_for_lib/ src/rules/mod.rs
git commit -m "feat(rust-thiserror-for-lib): flag pub enum Error without thiserror in lib crates"
```

---

## Batch 6 — TanStack Start

**Rules (4):** `tanstack-start-server-fn-requires-validation`, `tanstack-start-server-fn-requires-auth`, `tanstack-start-server-fn-file-convention`, `tanstack-start-require-validate-search`

---

### Task 6.1 — `tanstack-start-server-fn-requires-validation`

**Files:**
- Create: `src/rules/tanstack_start_server_fn_requires_validation/mod.rs`
- Create: `src/rules/tanstack_start_server_fn_requires_validation/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-requires-validation",
    description: "`createServerFn` handlers must validate their input with `.input()` or `.safeParse()`.",
    remediation: "Chain `.input(z.object({...}))` before `.handler(...)` to validate at the RPC boundary.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("createServerFn") { return vec![]; }
        if src.contains(".input(") || src.contains(".safeParse(") || src.contains(".parse(") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("createServerFn") && line.contains("(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` without `.input()` validation accepts unvalidated data at the RPC boundary.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("api.functions.ts"), src))
    }

    #[test]
    fn flags_no_input_validation() {
        assert_eq!(run("const fn = createServerFn().handler(async () => { await db.delete(x) })").len(), 1);
    }

    #[test]
    fn allows_with_input() {
        assert!(run("const fn = createServerFn().input(z.object({ id: z.string() })).handler(async (ctx) => {})").is_empty());
    }

    #[test]
    fn ignores_non_server_fn_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tanstack_start_server_fn_requires_validation
git add src/rules/tanstack_start_server_fn_requires_validation/ src/rules/mod.rs
git commit -m "feat(tanstack-start-server-fn-requires-validation): flag createServerFn without input validation"
```

---

### Task 6.2 — `tanstack-start-server-fn-requires-auth`

**Files:**
- Create: `src/rules/tanstack_start_server_fn_requires_auth/mod.rs`
- Create: `src/rules/tanstack_start_server_fn_requires_auth/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-requires-auth",
    description: "`createServerFn` handlers with DB mutations must verify authentication.",
    remediation: "Call `getSession()` or `auth()` at the top of the handler and throw if no session.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("createServerFn") { return vec![]; }
        if !src.contains(".insert(") && !src.contains(".update(") && !src.contains(".delete(") {
            return vec![];
        }
        if src.contains("getSession(") || src.contains("auth()") || src.contains("verifySession")
            || src.contains("requireAuth") || src.contains("currentUser(")
        {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("createServerFn") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`createServerFn` with mutations must verify authentication before proceeding.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("api.functions.ts"), src))
    }

    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(run("const del = createServerFn().handler(async () => { await db.delete(posts) })").len(), 1);
    }

    #[test]
    fn allows_with_get_session() {
        assert!(run("const del = createServerFn().handler(async () => { const s = await getSession(); await db.delete(posts) })").is_empty());
    }

    #[test]
    fn allows_read_only() {
        assert!(run("const get = createServerFn().handler(async () => db.select().from(posts))").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tanstack_start_server_fn_requires_auth
git add src/rules/tanstack_start_server_fn_requires_auth/ src/rules/mod.rs
git commit -m "feat(tanstack-start-server-fn-requires-auth): flag unauthenticated createServerFn mutations"
```

---

### Task 6.3 — `tanstack-start-server-fn-file-convention`

**Files:**
- Create: `src/rules/tanstack_start_server_fn_file_convention/mod.rs`
- Create: `src/rules/tanstack_start_server_fn_file_convention/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-server-fn-file-convention",
    description: "`createServerFn` must live in a `.functions.ts` file to enforce server/client separation.",
    remediation: "Move `createServerFn` calls to a file named `*.functions.ts`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("createServerFn") { return vec![]; }
        let file_name = ctx.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if file_name.ends_with(".functions.ts") || file_name.ends_with(".functions.tsx") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if line.contains("createServerFn") && line.contains("(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`createServerFn` must be in a `*.functions.ts` file, not `{file_name}`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                break; // once per file
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_wrong_file_name() {
        assert_eq!(run("src/users/actions.ts", "const fn = createServerFn()").len(), 1);
    }

    #[test]
    fn allows_functions_ts() {
        assert!(run("src/users/users.functions.ts", "const fn = createServerFn()").is_empty());
    }

    #[test]
    fn ignores_no_server_fn() {
        assert!(run("src/users/actions.ts", "const x = 1").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tanstack_start_server_fn_file_convention
git add src/rules/tanstack_start_server_fn_file_convention/ src/rules/mod.rs
git commit -m "feat(tanstack-start-server-fn-file-convention): enforce .functions.ts naming"
```

---

### Task 6.4 — `tanstack-start-require-validate-search`

**Files:**
- Create: `src/rules/tanstack_start_require_validate_search/mod.rs`
- Create: `src/rules/tanstack_start_require_validate_search/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-start-require-validate-search",
    description: "Routes calling `Route.useSearch()` must define `validateSearch:` on the route.",
    remediation: "Add `validateSearch: z.object({ ... })` to the `createFileRoute()` options.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tanstack-start"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("Route.useSearch(") && !src.contains("useSearch()") { return vec![]; }
        if src.contains("validateSearch:") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("Route.useSearch(") || (line.contains("useSearch(") && line.contains("Route")) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`Route.useSearch()` without `validateSearch:` in the route config accepts untyped search params.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("route.tsx"), src))
    }

    #[test]
    fn flags_use_search_without_validate() {
        assert_eq!(run("const { page } = Route.useSearch()").len(), 1);
    }

    #[test]
    fn allows_with_validate_search() {
        assert!(run("const { page } = Route.useSearch()\nconst route = createFileRoute('/posts')({ validateSearch: z.object({ page: z.number() }) })").is_empty());
    }

    #[test]
    fn ignores_no_use_search() {
        assert!(run("const route = createFileRoute('/posts')({})").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run tanstack_start_require_validate_search
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/tanstack_start_require_validate_search/ src/rules/mod.rs
git commit -m "feat(tanstack-start-require-validate-search): flag useSearch without validateSearch config"
```

---

## Batch 7 — TanStack Query

**Rules (10):** Mostly v5 API renames — all TextCheck. These are fast to implement; do them together in one session.

Registration pattern for all rules in this batch (TS + TSX only):
```rust
RuleDef {
    meta: META,
    backends: vec![
        (Language::TypeScript, Backend::Text(Box::new(text::Check))),
        (Language::Tsx, Backend::Text(Box::new(text::Check))),
    ],
}
```

---

### Task 7.1 — `tanstack-query-no-is-loading`

**Files:** `src/rules/tanstack_query_no_is_loading/{mod.rs,text.rs}`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "tanstack-query-no-is-loading",
    description: "`isLoading` was renamed to `isPending` in TanStack Query v5.",
    remediation: "Replace `isLoading` with `isPending` (or `isFetching` if you need network activity).",
    severity: Severity::Warning,
    doc_url: Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5"),
    categories: &["tanstack"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("useQuery") && !src.contains("useInfiniteQuery") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("isLoading") && !line.trim().starts_with("//") {
                if let Some(col) = line.find("isLoading") {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "`isLoading` was removed in TanStack Query v5 — use `isPending` instead.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src))
    }

    #[test]
    fn flags_is_loading() {
        assert_eq!(run("const { isLoading } = useQuery({ queryKey: ['x'], queryFn: f })").len(), 1);
    }

    #[test]
    fn allows_is_pending() {
        assert!(run("const { isPending } = useQuery({ queryKey: ['x'], queryFn: f })").is_empty());
    }

    #[test]
    fn ignores_file_without_usequery() {
        assert!(run("const isLoading = true").is_empty());
    }
}
```

- [x] **Step 3: Register + commit**
```bash
cargo nextest run tanstack_query_no_is_loading
git add src/rules/tanstack_query_no_is_loading/ src/rules/mod.rs
git commit -m "feat(tanstack-query-no-is-loading): flag removed isLoading API (v5)"
```

---

### Task 7.2 — `tanstack-query-no-cache-time`

**Files:** `src/rules/tanstack_query_no_cache_time/{mod.rs,text.rs}`

- [x] **mod.rs** — same pattern, id `"tanstack-query-no-cache-time"`, description: `` "`cacheTime` was renamed to `gcTime` in TanStack Query v5." ``, remediation: `"Replace `cacheTime` with `gcTime`."`, doc_url `Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5")`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("QueryClient") && !src.contains("useQuery") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.contains("cacheTime") && !t.starts_with("//") {
                if let Some(col) = line.find("cacheTime") {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "`cacheTime` was renamed to `gcTime` in TanStack Query v5.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }

    #[test]
    fn flags_cache_time() { assert_eq!(run("new QueryClient({ defaultOptions: { queries: { cacheTime: 5000 } } })").len(), 1); }
    #[test]
    fn allows_gc_time() { assert!(run("new QueryClient({ defaultOptions: { queries: { gcTime: 5000 } } })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-no-cache-time): flag renamed cacheTime API (v5)"`

---

### Task 7.3 — `tanstack-query-no-use-error-boundary`

**Files:** `src/rules/tanstack_query_no_use_error_boundary/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-no-use-error-boundary"`, description: `` "`useErrorBoundary` was removed in TanStack Query v5." ``, remediation: `"Use the `throwOnError` option instead."`, doc_url `Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5")`.

- [x] **text.rs** — scan for `useErrorBoundary` in files containing `useQuery`; flag each occurrence.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("useQuery") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if line.contains("useErrorBoundary") && !line.trim().starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("useErrorBoundary").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`useErrorBoundary` was removed in v5 — use `throwOnError` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags() { assert_eq!(run("useQuery({ queryKey: ['x'], queryFn: f, useErrorBoundary: true })").len(), 1); }
    #[test]
    fn allows() { assert!(run("useQuery({ queryKey: ['x'], queryFn: f, throwOnError: true })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-no-use-error-boundary): flag removed useErrorBoundary (v5)"`

---

### Task 7.4 — `tanstack-query-no-keep-previous-data-prop`

**Files:** `src/rules/tanstack_query_no_keep_previous_data_prop/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-no-keep-previous-data-prop"`, description: `` "`keepPreviousData: true` was replaced by `placeholderData: keepPreviousData` in v5." ``, remediation: `"Import `keepPreviousData` from `@tanstack/react-query` and use `placeholderData: keepPreviousData`."`, doc_url `Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5")`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if line.contains("keepPreviousData") && line.contains(": true") && !line.trim().starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("keepPreviousData").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`keepPreviousData: true` was removed in v5 — use `placeholderData: keepPreviousData` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags() { assert_eq!(run("useQuery({ queryKey: ['x'], queryFn: f, keepPreviousData: true })").len(), 1); }
    #[test]
    fn allows() { assert!(run("useQuery({ queryKey: ['x'], queryFn: f, placeholderData: keepPreviousData })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-no-keep-previous-data-prop): flag keepPreviousData: true (v5)"`

---

### Task 7.5 — `tanstack-query-no-query-callbacks`

**Files:** `src/rules/tanstack_query_no_query_callbacks/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-no-query-callbacks"`, description: `` "`onSuccess`/`onError`/`onSettled` callbacks on `useQuery` were removed in v5." ``, remediation: `"Move side-effects to `useEffect` watching the query result."`, doc_url `Some("https://tanstack.com/query/v5/docs/react/guides/migrating-to-v5")`.

- [x] **text.rs** — scan lines for `onSuccess:`, `onError:`, `onSettled:` in files with `useQuery` but NOT `useMutation`.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const REMOVED_CALLBACKS: &[&str] = &["onSuccess:", "onError:", "onSettled:"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        // Only files with useQuery (callbacks are still valid on useMutation)
        if !src.contains("useQuery") && !src.contains("useSuspenseQuery") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            for cb in REMOVED_CALLBACKS {
                if t.contains(cb) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: line.find(cb).unwrap_or(0) + 1,
                        rule_id: super::META.id.into(),
                        message: format!("`{cb}` on `useQuery` was removed in TanStack Query v5 — move side-effects to `useEffect`."),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_on_success() { assert_eq!(run("useQuery({ queryKey: ['x'], queryFn: f, onSuccess: () => {} })").len(), 1); }
    #[test]
    fn allows_no_callbacks() { assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty()); }
    #[test]
    fn ignores_no_usequery() { assert!(run("useMutation({ onSuccess: () => {} })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-no-query-callbacks): flag removed onSuccess/onError on useQuery (v5)"`

---

### Task 7.6 — `tanstack-query-require-stale-time`

**Files:** `src/rules/tanstack_query_require_stale_time/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-require-stale-time"`, description: `` "`QueryClient` without a default `staleTime` refetches on every mount." ``, remediation: `"Add `defaultOptions: { queries: { staleTime: 60_000 } }` to `QueryClient`."`, severity: `Warning`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("QueryClient") { return vec![]; }
        if src.contains("staleTime") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("new QueryClient(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`QueryClient` without a default `staleTime` refetches on every component mount.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_no_stale_time() { assert_eq!(run("const client = new QueryClient({})").len(), 1); }
    #[test]
    fn allows_stale_time() { assert!(run("const client = new QueryClient({ defaultOptions: { queries: { staleTime: 60_000 } } })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-require-stale-time): flag QueryClient without default staleTime"`

---

### Task 7.7 — `tanstack-query-fn-must-throw-on-error`

**Files:** `src/rules/tanstack_query_fn_must_throw_on_error/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-fn-must-throw-on-error"`, description: `` "`queryFn` must throw on HTTP errors so TanStack Query can retry and surface them." ``, remediation: `"Check `res.ok` and throw: `if (!res.ok) throw new Error(...)`."`.

- [x] **text.rs** — find `queryFn:` blocks with `fetch(` but no `res.ok` or `response.ok` check.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("queryFn") || !src.contains("fetch(") { return vec![]; }
        if src.contains("res.ok") || src.contains("response.ok") || src.contains(".ok)") {
            return vec![];
        }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("queryFn") && line.contains("fetch(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`queryFn` with `fetch()` must check `res.ok` and throw on error so TanStack Query can retry.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_fetch_no_ok_check() {
        assert_eq!(run("queryFn: async () => { const res = await fetch('/api'); return res.json() }").len(), 1);
    }
    #[test]
    fn allows_with_ok_check() {
        assert!(run("queryFn: async () => { const res = await fetch('/api'); if (!res.ok) throw new Error('err'); return res.json() }").is_empty());
    }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-fn-must-throw-on-error): flag queryFn with fetch but no res.ok check"`

---

### Task 7.8 — `tanstack-query-no-enabled-true`

**Files:** `src/rules/tanstack_query_no_enabled_true/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-no-enabled-true"`, description: `` "`enabled: true` is the default in TanStack Query and should be omitted." ``, remediation: `"Remove `enabled: true` — queries are enabled by default."`.

- [x] **text.rs** — scan for `enabled: true` or `enabled:true` in useQuery context.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("useQuery") && !ctx.source.contains("useSuspenseQuery") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.starts_with("//") && (t.contains("enabled: true") || t.contains("enabled:true")) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("enabled").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`enabled: true` is redundant — queries are enabled by default.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_enabled_true() { assert_eq!(run("useQuery({ queryKey: ['x'], queryFn: f, enabled: true })").len(), 1); }
    #[test]
    fn allows_enabled_condition() { assert!(run("useQuery({ queryKey: ['x'], queryFn: f, enabled: !!userId })").is_empty()); }
    #[test]
    fn allows_no_enabled() { assert!(run("useQuery({ queryKey: ['x'], queryFn: f })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-no-enabled-true): flag redundant enabled: true in query options"`

---

### Task 7.9 — `tanstack-query-prefer-query-options`

**Files:** `src/rules/tanstack_query_prefer_query_options/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-prefer-query-options"`, description: `"Inline `queryKey`/`queryFn` objects should be extracted to `queryOptions()` factories for reuse."`, remediation: `"Use `queryOptions({ queryKey: [...], queryFn: ... })` and import the factory where needed."`.

- [x] **text.rs** — flag `useQuery({ queryKey:` (inline options) when `queryOptions(` is absent from the file.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("useQuery") { return vec![]; }
        if src.contains("queryOptions(") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("useQuery({") || (line.contains("useQuery(") && line.contains("queryKey:")) {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Extract inline `useQuery` options to a `queryOptions()` factory for reuse and type-safety.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_inline_options() { assert_eq!(run("useQuery({ queryKey: ['users'], queryFn: fetchUsers })").len(), 1); }
    #[test]
    fn allows_query_options_factory() { assert!(run("const opts = queryOptions({ queryKey: ['users'], queryFn: fetchUsers })\nuseQuery(opts)").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(tanstack-query-prefer-query-options): flag inline useQuery options without queryOptions factory"`

---

### Task 7.10 — `tanstack-query-prefer-key-factory`

**Files:** `src/rules/tanstack_query_prefer_key_factory/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"tanstack-query-prefer-key-factory"`, description: `"Inline dynamic `queryKey` arrays should use a key factory for consistency."`, remediation: `"Define a key factory: `const todoKeys = { detail: (id: string) => ['todos', id] as const }` and use `todoKeys.detail(id)`."`.

- [x] **text.rs** — flag `queryKey: [` arrays that contain both a string literal and a variable (mixed static/dynamic key).

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.contains("queryKey:") || !t.contains('[') { continue; }
            // Look for pattern: queryKey: ['string', variable] — string literal + identifier
            if let Some(bracket_start) = t.find("queryKey:") {
                let after = &t[bracket_start..];
                if let Some(arr_start) = after.find('[') {
                    if let Some(arr_end) = after.find(']') {
                        let arr = &after[arr_start + 1..arr_end];
                        // Has a quoted string AND a non-quoted token (variable)
                        let has_string = arr.contains('\'') || arr.contains('"');
                        let parts: Vec<&str> = arr.split(',').collect();
                        let has_var = parts.iter().any(|p| {
                            let p = p.trim();
                            !p.is_empty() && !p.starts_with('\'') && !p.starts_with('"')
                        });
                        if has_string && has_var {
                            diags.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: i + 1,
                                column: line.find("queryKey").unwrap_or(0) + 1,
                                rule_id: super::META.id.into(),
                                message: "Extract dynamic `queryKey` to a key factory: `const keys = { detail: (id) => ['res', id] as const }`.".into(),
                                severity: Severity::Warning,
                                span: None,
                            });
                        }
                    }
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_inline_dynamic_key() { assert_eq!(run("useQuery({ queryKey: ['todos', userId], queryFn: f })").len(), 1); }
    #[test]
    fn allows_static_key() { assert!(run("useQuery({ queryKey: ['todos'], queryFn: f })").is_empty()); }
    #[test]
    fn allows_factory() { assert!(run("useQuery({ queryKey: todoKeys.detail(userId), queryFn: f })").is_empty()); }
}
```

- [x] **Register all 10 rules + full suite**
```bash
cargo nextest run tanstack_query
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/tanstack_query_prefer_key_factory/ src/rules/mod.rs
git commit -m "feat(tanstack-query-prefer-key-factory): flag inline dynamic queryKey arrays"
```

---

## Batch 8 — API Design

**Rules (3):** `api-no-array-root-response`, `api-list-requires-pagination`, `api-import-from-public-index`

**Pre-implementation check:** Run `grep -r "layer-import-boundary\|api-import" src/rules/` to verify `api-import-from-public-index` doesn't overlap with existing rules.

---

### Task 8.1 — `api-no-array-root-response`

**Files:**
- Create: `src/rules/api_no_array_root_response/mod.rs`
- Create: `src/rules/api_no_array_root_response/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-array-root-response",
    description: "API endpoints must not return a root-level JSON array — wrap in an object for extensibility.",
    remediation: "Return `{ data: [...], total: n }` instead of a bare array.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ARRAY_RESPONSE_PATTERNS: &[&str] = &[
    "Response.json([", "res.json([", "c.json([", "return json([",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            for pattern in ARRAY_RESPONSE_PATTERNS {
                if line.contains(pattern) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: line.find(pattern).unwrap_or(0) + 1,
                        rule_id: super::META.id.into(),
                        message: "Return `{ data: [...] }` instead of a root-level array — arrays can't be extended without breaking clients.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("route.ts"), src)) }
    #[test]
    fn flags_response_json_array() { assert_eq!(run("export async function GET() { return Response.json([...users]) }").len(), 1); }
    #[test]
    fn allows_object_response() { assert!(run("export async function GET() { return Response.json({ data: users }) }").is_empty()); }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run api_no_array_root_response
git add src/rules/api_no_array_root_response/ src/rules/mod.rs
git commit -m "feat(api-no-array-root-response): flag root-level array JSON responses"
```

---

### Task 8.2 — `api-list-requires-pagination`

**Files:**
- Create: `src/rules/api_list_requires_pagination/mod.rs`
- Create: `src/rules/api_list_requires_pagination/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs** — id `"api-list-requires-pagination"`, description: `"List endpoints must support pagination to prevent unbounded result sets."`, remediation: `"Add `limit`/`cursor` or `page`/`pageSize` parameters to the handler."`, categories: `&["api"]`.

- [x] **Step 2: Create text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PAGINATION_TERMS: &[&str] = &["limit", "cursor", "page", "offset", "pageSize", "per_page"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        // Only files that look like API route handlers
        if !src.contains("export async function GET") && !src.contains("export const GET") { return vec![]; }
        if PAGINATION_TERMS.iter().any(|p| src.contains(p)) { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("export async function GET") || t.starts_with("export const GET") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "GET handler has no pagination — add `limit`/`cursor` or `page`/`pageSize` to prevent unbounded queries.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("route.ts"), src)) }
    #[test]
    fn flags_get_without_pagination() { assert_eq!(run("export async function GET() { return db.select().from(users) }").len(), 1); }
    #[test]
    fn allows_get_with_limit() { assert!(run("export async function GET(req: Request) { const { limit } = await req.json(); return db.select().from(users).limit(limit) }").is_empty()); }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run api_list_requires_pagination
git add src/rules/api_list_requires_pagination/ src/rules/mod.rs
git commit -m "feat(api-list-requires-pagination): flag GET handlers without pagination"
```

---

### Task 8.3 — `api-import-from-public-index`

**Pre-check:** `grep -r "layer-import" src/rules/` — skip this rule if it duplicates existing coverage.

**Files:**
- Create: `src/rules/api_import_from_public_index/mod.rs`
- Create: `src/rules/api_import_from_public_index/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **Step 1: Create mod.rs** — id `"api-import-from-public-index"`, description: `"Cross-feature imports must go through the public index, not internal files."`, remediation: `"Import from `../users` (index) instead of `../users/db/queries`."`, categories: `&["api", "architecture"]`.

- [x] **Step 2: Create typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" { return; }
    let source_node = match node.child_by_field_name("source") {
        Some(s) => s,
        None => return,
    };
    let import_path = source_node.utf8_text(source).unwrap_or("").trim_matches(|c| c == '\'' || c == '"');

    // Only cross-feature imports (2+ parent segments)
    let parent_count = import_path.split('/').filter(|s| *s == "..").count();
    if parent_count < 2 { return; }

    // Flag if the import doesn't end at an index file
    let last_segment = import_path.split('/').last().unwrap_or("");
    if last_segment == "index" || last_segment.is_empty() { return; }
    // Skip if it's clearly a types/utils import
    if last_segment == "types" || last_segment == "utils" { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &source_node,
        super::META.id,
        format!("Import from `{import_path}` crosses a feature boundary — import from the public index instead."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }

    #[test]
    fn flags_deep_cross_feature_import() {
        assert_eq!(run("import { query } from '../../users/db/queries'").len(), 1);
    }

    #[test]
    fn allows_index_import() {
        assert!(run("import { User } from '../../users'").is_empty());
    }

    #[test]
    fn allows_single_parent() {
        assert!(run("import { helper } from '../utils/format'").is_empty());
    }
}
```

- [x] **Step 3: Register + test + commit**
```bash
cargo nextest run api_import_from_public_index
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/api_import_from_public_index/ src/rules/mod.rs
git commit -m "feat(api-import-from-public-index): flag deep cross-feature imports bypassing public index"
```

---

## Batch 9 — Zod

**Rules (7):** `zod-prefer-safe-parse`, `zod-string-min-1-required`, `zod-trim-before-min`, `zod-prefer-discriminated-union`, `zod-refine-requires-path`, `zod-require-error-messages`, `zod-no-optional-nullable-chain`

---

### Task 9.1 — `zod-prefer-safe-parse`

**Files:**
- Create: `src/rules/zod_prefer_safe_parse/mod.rs`
- Create: `src/rules/zod_prefer_safe_parse/text.rs`
- Modify: `src/rules/mod.rs`

- [x] **mod.rs** — id `"zod-prefer-safe-parse"`, description: `"`.parse()` in a route handler throws `ZodError` unhandled — use `.safeParse()` instead."`, remediation: `"Use `.safeParse()` and handle `!result.success` to return a structured 400 response."`, categories: `&["zod", "api"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ROUTE_FILE_PATTERNS: &[&str] = &["route.ts", "route.tsx", "handler.ts", "+server.ts", "page.server.ts", "controller.ts"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_route = ROUTE_FILE_PATTERNS.iter().any(|p| file_name.ends_with(p))
            || ctx.source.contains("export async function GET")
            || ctx.source.contains("export async function POST")
            || ctx.source.contains("export async function PUT")
            || ctx.source.contains("export async function DELETE");
        if !is_route { return vec![]; }
        if !ctx.source.contains(".parse(") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            // .parse( but not .safeParse(
            if t.contains(".parse(") && !t.contains(".safeParse(") && !t.contains("JSON.parse(") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find(".parse(").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "Use `.safeParse()` in route handlers — `.parse()` throws `ZodError` which leaks schema internals to clients.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(path: &str, src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new(path), src)) }
    #[test]
    fn flags_parse_in_route() { assert_eq!(run("route.ts", "export async function POST() { const body = schema.parse(data) }").len(), 1); }
    #[test]
    fn allows_safe_parse() { assert!(run("route.ts", "const r = schema.safeParse(data)").is_empty()); }
    #[test]
    fn allows_json_parse() { assert!(run("route.ts", "export async function POST() { const body = JSON.parse(raw) }").is_empty()); }
    #[test]
    fn ignores_non_route() { assert!(run("utils.ts", "const x = schema.parse(data)").is_empty()); }
}
```

- [x] **Register + test + commit**
```bash
cargo nextest run zod_prefer_safe_parse
git add src/rules/zod_prefer_safe_parse/ src/rules/mod.rs
git commit -m "feat(zod-prefer-safe-parse): flag .parse() in route handlers"
```

---

### Task 9.2 — `zod-string-min-1-required`

**Files:** `src/rules/zod_string_min_1_required/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"zod-string-min-1-required"`, description: `` "Bare `z.string()` without length constraints accepts empty strings." ``, remediation: `"Add `.min(1)` or `.trim().min(1)` to reject empty strings."`, categories: `&["zod"]`.

- [x] **text.rs** — find `z.string()` not followed by length/format chain on the same line.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const VALID_CONTINUATIONS: &[&str] = &[
    ".min(", ".max(", ".email(", ".url(", ".uuid(", ".regex(", ".length(",
    ".startsWith(", ".endsWith(", ".optional(", ".nullable(", ".nullish(",
    ".trim(", ".toLowerCase(", ".toUpperCase(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("z.string()") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("z.string()") { continue; }
            if line.trim().starts_with("//") { continue; }
            if let Some(pos) = line.find("z.string()") {
                let after = &line[pos + "z.string()".len()..];
                if VALID_CONTINUATIONS.iter().any(|c| after.contains(c)) { continue; }
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: pos + 1,
                    rule_id: super::META.id.into(),
                    message: "Bare `z.string()` accepts empty strings — add `.min(1)` or a format constraint.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("schema.ts"), src)) }
    #[test]
    fn flags_bare_string() { assert_eq!(run("const s = z.object({ name: z.string() })").len(), 1); }
    #[test]
    fn allows_min() { assert!(run("z.string().min(1)").is_empty()); }
    #[test]
    fn allows_email() { assert!(run("z.string().email()").is_empty()); }
    #[test]
    fn allows_optional() { assert!(run("z.string().optional()").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(zod-string-min-1-required): flag bare z.string() without length constraint"`

---

### Task 9.3 — `zod-trim-before-min`

**Files:** `src/rules/zod_trim_before_min/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"zod-trim-before-min"`, description: `` "`z.string().min(1)` without `.trim()` allows strings of only whitespace." ``, remediation: `"Add `.trim()` before `.min(1)`: `z.string().trim().min(1)`."`, categories: `&["zod"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            if !line.contains("z.string()") || !line.contains(".min(") { continue; }
            if line.trim().starts_with("//") { continue; }
            if line.contains(".trim()") { continue; }
            if let Some(pos) = line.find("z.string()") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: pos + 1,
                    rule_id: super::META.id.into(),
                    message: "Add `.trim()` before `.min()` — `z.string().min(1)` allows whitespace-only strings.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_min_without_trim() { assert_eq!(run("z.string().min(1)").len(), 1); }
    #[test]
    fn allows_trim_before_min() { assert!(run("z.string().trim().min(1)").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(zod-trim-before-min): flag z.string().min() without prior .trim()"`

---

### Task 9.4 — `zod-prefer-discriminated-union`

**Files:** `src/rules/zod_prefer_discriminated_union/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"zod-prefer-discriminated-union"`, description: `` "`z.union([z.object({...}), ...])` with shared discriminant fields should use `z.discriminatedUnion()`." ``, remediation: `"Use `z.discriminatedUnion('type', [...])` for faster parsing and better error messages."`, categories: `&["zod"]`.

- [x] **text.rs** — find `z.union([z.object({` where inner objects contain `type: z.literal(` or `kind: z.literal(`.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        let mut in_union = false;
        let mut union_start = 0;
        let mut has_literal = false;

        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            if t.contains("z.union([") && !t.contains("z.discriminatedUnion") {
                in_union = true;
                union_start = i;
                has_literal = false;
            }
            if in_union && (t.contains("type: z.literal(") || t.contains("kind: z.literal(") || t.contains("__type: z.literal(")) {
                has_literal = true;
            }
            if in_union && t.contains("])") {
                if has_literal {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: union_start + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Replace `z.union([z.object({type: z.literal(...)}), ...])` with `z.discriminatedUnion('type', [...])` for faster parsing.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                in_union = false;
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_union_with_literals() {
        let src = "z.union([\n  z.object({ type: z.literal('a') }),\n  z.object({ type: z.literal('b') }),\n])";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_discriminated_union() {
        assert!(run("z.discriminatedUnion('type', [z.object({ type: z.literal('a') })])").is_empty());
    }
}
```

- [x] **Register + commit:** `git commit -m "feat(zod-prefer-discriminated-union): flag z.union with literal discriminant fields"`

---

### Task 9.5 — `zod-refine-requires-path`

**Files:** `src/rules/zod_refine_requires_path/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"zod-refine-requires-path"`, description: `` "`z.object().refine()` without `path:` attaches the error to the whole object, not a specific field." ``, remediation: `"Add `path: ['fieldName']` to the refine options so form errors appear on the correct field."`, categories: `&["zod"]`.

- [x] **text.rs** — find `.refine(` on lines that also have `z.object(` context (or preceded by one), where the refine call lacks `path:`.

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("z.object(") || !ctx.source.contains(".refine(") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.contains(".refine(") || t.starts_with("//") { continue; }
            if t.contains("path:") || t.contains("path :") { continue; }
            // Only flag when it looks like a cross-field refine (has two arguments to refine)
            // Heuristic: refine with object literal as second arg without path:
            if t.contains(".refine(") && t.contains("message") && !t.contains("path") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find(".refine(").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "Add `path: ['fieldName']` to `.refine()` options so form errors attach to the correct field.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_refine_no_path() {
        assert_eq!(run("z.object({ a: z.string(), b: z.string() }).refine(d => d.a !== d.b, { message: 'Must differ' })").len(), 1);
    }
    #[test]
    fn allows_refine_with_path() {
        assert!(run("z.object({ a: z.string() }).refine(d => d.a.length > 0, { message: 'Required', path: ['a'] })").is_empty());
    }
}
```

- [x] **Register + commit:** `git commit -m "feat(zod-refine-requires-path): flag .refine() without path option on object schemas"`

---

### Task 9.6 — `zod-require-error-messages`

**Files:** `src/rules/zod_require_error_messages/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"zod-require-error-messages"`, description: `` "`.refine()` without an error message produces unhelpful validation errors." ``, remediation: `"Add `{ message: 'descriptive error' }` as the second argument to `.refine()`."`, categories: `&["zod"]`.

- [x] **text.rs** — find `.refine(` with a single-argument call (closing `)` on same line after the function, no second arg).

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.contains(".refine(") || t.starts_with("//") { continue; }
            if t.contains("message") || t.contains("{ message") { continue; }
            // Single-arg refine ends with `})` or `})` after arrow fn
            if t.contains(".refine(") && (t.ends_with(")") || t.ends_with(");") || t.ends_with("),")) {
                let after_refine = t.split(".refine(").nth(1).unwrap_or("");
                // Count commas at top level — if 0, it's a single-arg call
                let mut depth = 0usize;
                let mut comma_count = 0;
                for c in after_refine.chars() {
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => { if depth > 0 { depth -= 1; } }
                        ',' if depth == 0 => comma_count += 1,
                        _ => {}
                    }
                }
                if comma_count == 0 {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: line.find(".refine(").unwrap_or(0) + 1,
                        rule_id: super::META.id.into(),
                        message: "Add `{ message: '...' }` to `.refine()` — bare refine produces no helpful error message.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_single_arg_refine() { assert_eq!(run("z.string().refine(val => val.includes('@'))").len(), 1); }
    #[test]
    fn allows_refine_with_message() { assert!(run("z.string().refine(val => val.includes('@'), { message: 'Must be email' })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(zod-require-error-messages): flag .refine() without error message"`

---

### Task 9.7 — `zod-no-optional-nullable-chain`

**Files:** `src/rules/zod_no_optional_nullable_chain/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"zod-no-optional-nullable-chain"`, description: `` "`.optional().nullable()` should be written as `.nullish()` for clarity." ``, remediation: `"Replace `.optional().nullable()` or `.nullable().optional()` with `.nullish()`."`, categories: `&["zod"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            let has_chain = t.contains(".optional().nullable()") || t.contains(".nullable().optional()");
            if has_chain {
                let col = line.find(".optional().nullable()").or_else(|| line.find(".nullable().optional()")).unwrap_or(0);
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "Replace `.optional().nullable()` with `.nullish()` for clearer intent.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.ts"), src)) }
    #[test]
    fn flags_optional_nullable() { assert_eq!(run("z.string().optional().nullable()").len(), 1); }
    #[test]
    fn flags_nullable_optional() { assert_eq!(run("z.string().nullable().optional()").len(), 1); }
    #[test]
    fn allows_nullish() { assert!(run("z.string().nullish()").is_empty()); }
}
```

- [x] **Register all Zod rules + full suite**
```bash
cargo nextest run zod_
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/zod_no_optional_nullable_chain/ src/rules/mod.rs
git commit -m "feat(zod-no-optional-nullable-chain): flag .optional().nullable() chain"
```

---

## Batch 10 — Vue

**Rules (7):** `vue-script-setup-required`, `vue-sfc-section-order`, `vue-no-v-html-unsafe`, `vue-prefer-v-else`, `vue-require-lifecycle-cleanup`, `vue-pinia-store-to-refs`, `vue-define-emits-typed`

All Vue rules use TextCheck + `Language::Vue` only. See `vue_no_reactive_destructure` for the canonical registration pattern.

---

### Task 10.1 — `vue-script-setup-required`

**Files:** `src/rules/vue_script_setup_required/{mod.rs,text.rs}`

- [x] **mod.rs**

```rust
mod text;
use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "vue-script-setup-required",
    description: "`<script>` without `setup` attribute uses Options-API-style Composition API — use `<script setup>` instead.",
    remediation: "Change `<script lang=\"ts\">` to `<script setup lang=\"ts\">` and remove the `setup()` function.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["vue"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}
```

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if src.contains("<script setup") { return vec![]; }
        if !src.contains("setup()") && !src.contains("setup(props") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if (t.starts_with("<script>") || t.starts_with("<script lang=")) && !t.contains("setup") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "Use `<script setup>` instead of `<script>` with a `setup()` function.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                break;
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_script_with_setup_fn() { assert_eq!(run("<script lang=\"ts\">\nexport default { setup() { return {} } }\n</script>").len(), 1); }
    #[test]
    fn allows_script_setup() { assert!(run("<script setup lang=\"ts\">\nconst x = 1\n</script>").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(vue-script-setup-required): flag <script> with setup() fn instead of <script setup>"`

---

### Task 10.2 — `vue-sfc-section-order`

**Files:** `src/rules/vue_sfc_section_order/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"vue-sfc-section-order"`, description: `"SFC sections must be ordered: `<script setup>` → `<template>` → `<style>`."`, remediation: `"Reorder sections: script first, template second, style last."`, categories: `&["vue"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut template_line: Option<usize> = None;
        let mut script_line: Option<usize> = None;
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("<template") && template_line.is_none() { template_line = Some(i); }
            if t.starts_with("<script") && script_line.is_none() { script_line = Some(i); }
        }
        match (template_line, script_line) {
            (Some(tl), Some(sl)) if tl < sl => {
                vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: tl + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`<template>` appears before `<script>` — the canonical SFC order is: script → template → style.".into(),
                    severity: Severity::Warning,
                    span: None,
                }]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_template_before_script() { assert_eq!(run("<template><div /></template>\n<script setup lang=\"ts\">\n</script>").len(), 1); }
    #[test]
    fn allows_script_before_template() { assert!(run("<script setup lang=\"ts\">\n</script>\n<template><div /></template>").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(vue-sfc-section-order): flag template before script in SFC"`

---

### Task 10.3 — `vue-no-v-html-unsafe`

**Files:** `src/rules/vue_no_v_html_unsafe/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"vue-no-v-html-unsafe"`, description: `` "`v-html` without sanitization is an XSS vector." ``, remediation: `"Wrap the value in `DOMPurify.sanitize(...)` before binding with `v-html`."`, severity: `Severity::Error`, categories: `&["vue", "security"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("v-html") { continue; }
            if line.contains("sanitize(") || line.contains("DOMPurify") { continue; }
            let prev_has_sanitize = i > 0 && (lines[i - 1].contains("sanitize") || lines[i - 1].contains("// safe"));
            if !prev_has_sanitize {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("v-html").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "`v-html` without sanitization is an XSS risk. Wrap the value in `DOMPurify.sanitize(...)`.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_v_html_no_sanitize() { assert_eq!(run("<div v-html=\"userContent\" />").len(), 1); }
    #[test]
    fn allows_v_html_with_sanitize() { assert!(run("<div v-html=\"DOMPurify.sanitize(content)\" />").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(vue-no-v-html-unsafe): flag v-html without DOMPurify sanitization"`

---

### Task 10.4 — `vue-prefer-v-else`

**Files:** `src/rules/vue_prefer_v_else/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"vue-prefer-v-else"`, description: `` "Consecutive `v-if=\"X\"` and `v-if=\"!X\"` should use `v-else`." ``, remediation: `"Replace the second `v-if=\"!X\"` with `v-else`."`, categories: `&["vue"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diags = Vec::new();
        for i in 0..lines.len().saturating_sub(1) {
            let cur = lines[i].trim();
            let next = lines[i + 1].trim();
            if !cur.contains("v-if=") || !next.contains("v-if=") { continue; }
            // Extract condition from v-if="..."
            if let (Some(cur_cond), Some(next_cond)) = (extract_v_if(cur), extract_v_if(next)) {
                if next_cond == format!("!{cur_cond}") || next_cond == format!("!({cur_cond})") {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 2,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!("Replace `v-if=\"{next_cond}\"` with `v-else` since the previous element uses `v-if=\"{cur_cond}\"`.")  ,
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

fn extract_v_if(line: &str) -> Option<String> {
    let start = line.find("v-if=\"")?;
    let after = &line[start + 6..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_negated_v_if() { assert_eq!(run("<div v-if=\"show\" />\n<div v-if=\"!show\" />").len(), 1); }
    #[test]
    fn allows_v_else() { assert!(run("<div v-if=\"show\" />\n<div v-else />").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(vue-prefer-v-else): flag consecutive v-if/!v-if that should use v-else"`

---

### Task 10.5 — `vue-require-lifecycle-cleanup`

**Files:** `src/rules/vue_require_lifecycle_cleanup/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"vue-require-lifecycle-cleanup"`, description: `` "`onMounted` with `addEventListener` must have a matching `onUnmounted` with `removeEventListener`." ``, remediation: `"Add `onUnmounted(() => element.removeEventListener(...))` to clean up."`, categories: `&["vue"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("onMounted") || !src.contains("addEventListener(") { return vec![]; }
        if src.contains("removeEventListener(") { return vec![]; }
        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.contains("addEventListener(") && !line.trim().starts_with("//") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`addEventListener` in `onMounted` without `removeEventListener` in `onUnmounted` leaks listeners.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_no_remove() {
        assert_eq!(run("onMounted(() => { window.addEventListener('resize', handler) })").len(), 1);
    }
    #[test]
    fn allows_with_remove() {
        assert!(run("onMounted(() => { window.addEventListener('resize', h) })\nonUnmounted(() => { window.removeEventListener('resize', h) })").is_empty());
    }
}
```

- [x] **Register + commit:** `git commit -m "feat(vue-require-lifecycle-cleanup): flag addEventListener without removeEventListener cleanup"`

---

### Task 10.6 — `vue-pinia-store-to-refs`

**Files:** `src/rules/vue_pinia_store_to_refs/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"vue-pinia-store-to-refs"`, description: `` "Destructuring a Pinia store without `storeToRefs()` loses reactivity." ``, remediation: `"Use `const { count } = storeToRefs(useCounterStore())` to preserve reactivity."`, categories: `&["vue"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            // Pattern: const { ... } = use*Store()
            if t.starts_with("const {") && t.contains("= use") && t.contains("Store()") {
                if !t.contains("storeToRefs(") {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Wrap the store in `storeToRefs()` when destructuring to preserve reactivity.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_destructure_without_store_to_refs() { assert_eq!(run("const { count, name } = useCounterStore()").len(), 1); }
    #[test]
    fn allows_store_to_refs() { assert!(run("const { count } = storeToRefs(useCounterStore())").is_empty()); }
    #[test]
    fn allows_no_destructure() { assert!(run("const store = useCounterStore()").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(vue-pinia-store-to-refs): flag Pinia store destructuring without storeToRefs"`

---

### Task 10.7 — `vue-define-emits-typed`

**Files:** `src/rules/vue_define_emits_typed/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"vue-define-emits-typed"`, description: `` "`defineEmits([...])` array form loses type safety — use the generic `defineEmits<{...}>()` form." ``, remediation: `"Use `defineEmits<{ change: [value: string] }>()` for full type-checking on emits."`, categories: `&["vue"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            // Array form: defineEmits([  — not generic form: defineEmits<
            if t.contains("defineEmits([") {
                diags.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: line.find("defineEmits").unwrap_or(0) + 1,
                    rule_id: super::META.id.into(),
                    message: "Use `defineEmits<{ eventName: [arg: Type] }>()` instead of the untyped array form.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src)) }
    #[test]
    fn flags_array_form() { assert_eq!(run("const emit = defineEmits(['change', 'update'])").len(), 1); }
    #[test]
    fn allows_typed_form() { assert!(run("const emit = defineEmits<{ change: [value: string] }>()").is_empty()); }
}
```

- [x] **Register all Vue rules + full suite**
```bash
cargo nextest run vue_
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/vue_define_emits_typed/ src/rules/mod.rs
git commit -m "feat(vue-define-emits-typed): flag untyped defineEmits([]) array form"
```

---

## Batch 11 — i18n

**Rules (5):** `i18n-no-hardcoded-string-in-jsx`, `i18n-no-concat-translation-key`, `i18n-no-string-concat-with-translation`, `i18n-prefer-intl-api`, `i18n-no-manual-pluralization`

---

### Task 11.1 — `i18n-no-hardcoded-string-in-jsx`

**Files:**
- Create: `src/rules/i18n_no_hardcoded_string_in_jsx/mod.rs`
- Create: `src/rules/i18n_no_hardcoded_string_in_jsx/typescript.rs`
- Modify: `src/rules/mod.rs`

- [x] **mod.rs**

```rust
mod typescript;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "i18n-no-hardcoded-string-in-jsx",
    description: "Hardcoded string literals in JSX text content won't be translated.",
    remediation: "Wrap the string with the `t()` translation function.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["i18n"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
```

- [x] **typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // jsx_text nodes are the text content between JSX tags
    if node.kind() != "jsx_text" { return; }
    let text = node.utf8_text(source).unwrap_or("").trim();
    // Skip whitespace-only, single chars, or strings without spaces (technical)
    if text.is_empty() || !text.contains(' ') || text.len() <= 2 { return; }
    // Skip if it looks like a number or punctuation only
    if text.chars().all(|c| c.is_ascii_digit() || c.is_ascii_punctuation()) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Hardcoded string \"{text}\" in JSX — wrap with `t()`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_tsx(s, &Check) }

    #[test]
    fn flags_text_content() { assert_eq!(run("<div>Hello World</div>").len(), 1); }
    #[test]
    fn flags_paragraph() { assert_eq!(run("<p>Submit your application</p>").len(), 1); }
    #[test]
    fn allows_translation_call() { assert!(run("<div>{t('home.greeting')}</div>").is_empty()); }
    #[test]
    fn allows_whitespace_only() { assert!(run("<div> </div>").is_empty()); }
    #[test]
    fn allows_single_char() { assert!(run("<span>:</span>").is_empty()); }
}
```

- [x] **Register + test + commit**
```bash
cargo nextest run i18n_no_hardcoded_string_in_jsx
git add src/rules/i18n_no_hardcoded_string_in_jsx/ src/rules/mod.rs
git commit -m "feat(i18n-no-hardcoded-string-in-jsx): flag literal text content in JSX elements"
```

---

### Task 11.2 — `i18n-no-concat-translation-key`

**Files:** `src/rules/i18n_no_concat_translation_key/{mod.rs,typescript.rs}`

- [x] **mod.rs** — id `"i18n-no-concat-translation-key"`, description: `` "Dynamic `t()` keys built with concatenation or template literals can't be statically extracted." ``, remediation: `"Use full static key strings: `t('section.home')` instead of `t('section.' + name)`."`, categories: `&["i18n"]`.

- [x] **typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    let func_text = func.utf8_text(source).unwrap_or("");
    if func_text != "t" && func_text != "i18n.t" { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    let first = match args.named_child(0) {
        Some(a) => a,
        None => return,
    };
    // Flag template literals and binary expressions as the first arg
    if first.kind() == "template_string" || first.kind() == "binary_expression" {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &first,
            super::META.id,
            "Dynamic `t()` key can't be statically extracted by i18next — use a full static key string.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }
    #[test]
    fn flags_concat_key() { assert_eq!(run("t('section.' + name)").len(), 1); }
    #[test]
    fn flags_template_key() { assert_eq!(run("t(`nav.${route}`)").len(), 1); }
    #[test]
    fn allows_static_key() { assert!(run("t('section.home')").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(i18n-no-concat-translation-key): flag dynamic t() key concatenation"`

---

### Task 11.3 — `i18n-no-string-concat-with-translation`

**Files:** `src/rules/i18n_no_string_concat_with_translation/{mod.rs,text.rs}`

- [x] **mod.rs** — id `"i18n-no-string-concat-with-translation"`, description: `` "Concatenating `t()` results breaks word order in RTL and agglutinative languages." ``, remediation: `"Use interpolation: `t('greeting', { name })` instead of `t('hello') + ' ' + name`."`, categories: `&["i18n"]`.

- [x] **text.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diags = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if t.starts_with("//") { continue; }
            // t('key') + or + t('key')
            if (t.contains("t('") || t.contains("t(\"")) && t.contains(" + ") {
                if let Some(col) = line.find(" + ").filter(|_| line.contains("t('") || line.contains("t(\"")) {
                    diags.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: i + 1,
                        column: col + 1,
                        rule_id: super::META.id.into(),
                        message: "Don't concatenate `t()` results — use interpolation variables in the translation string instead.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(src: &str) -> Vec<Diagnostic> { Check.check(&CheckCtx::for_test(Path::new("t.tsx"), src)) }
    #[test]
    fn flags_concat() { assert_eq!(run("const msg = t('hello') + ' ' + name").len(), 1); }
    #[test]
    fn allows_interpolation() { assert!(run("const msg = t('greeting', { name })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(i18n-no-string-concat-with-translation): flag t() concatenation"`

---

### Task 11.4 — `i18n-prefer-intl-api`

**Files:** `src/rules/i18n_prefer_intl_api/{mod.rs,typescript.rs}`

- [x] **mod.rs** — id `"i18n-prefer-intl-api"`, description: `` "`.toLocaleDateString()` without an explicit locale uses the environment default, which varies by machine." ``, remediation: `"Pass `i18n.language` as the first argument or use `Intl.DateTimeFormat(locale).format(date)`."`, categories: `&["i18n"]`.

- [x] **typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

const LOCALE_METHODS: &[&str] = &["toLocaleDateString", "toLocaleTimeString", "toLocaleString"];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let func = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return,
    };
    if func.kind() != "member_expression" { return; }
    let prop = match func.child_by_field_name("property") {
        Some(p) => p,
        None => return,
    };
    let method = prop.utf8_text(source).unwrap_or("");
    if !LOCALE_METHODS.contains(&method) { return; }

    let args = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return,
    };
    // Flag if no arguments (no locale passed)
    if args.named_child_count() == 0 {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("Pass an explicit locale to `.{method}()` — without one, formatting depends on the environment locale."),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }
    #[test]
    fn flags_no_locale() { assert_eq!(run("date.toLocaleDateString()").len(), 1); }
    #[test]
    fn flags_tolocalestring_no_args() { assert_eq!(run("n.toLocaleString()").len(), 1); }
    #[test]
    fn allows_with_locale() { assert!(run("date.toLocaleDateString(i18n.language, { dateStyle: 'short' })").is_empty()); }
}
```

- [x] **Register + commit:** `git commit -m "feat(i18n-prefer-intl-api): flag .toLocaleDateString() without explicit locale"`

---

### Task 11.5 — `i18n-no-manual-pluralization`

**Files:** `src/rules/i18n_no_manual_pluralization/{mod.rs,typescript.rs}`

- [x] **mod.rs** — id `"i18n-no-manual-pluralization"`, description: `` "Manual `count === 1 ? singular : plural` ignores CLDR plural rules for non-English languages." ``, remediation: `"Use `t('key', { count })` — i18next applies CLDR plural rules automatically."`, categories: `&["i18n"]`.

- [x] **typescript.rs**

```rust
use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "ternary_expression" { return; }
    let condition = match node.child_by_field_name("condition") {
        Some(c) => c,
        None => return,
    };
    let cond_text = condition.utf8_text(source).unwrap_or("");
    // Condition must be count-related comparison
    if !cond_text.contains("count") && !cond_text.contains("length") && !cond_text.contains(".size") {
        return;
    }
    if !cond_text.contains("=== 1") && !cond_text.contains("== 1") && !cond_text.contains("> 1") {
        return;
    }
    // Both branches should be t() calls
    let consequence = match node.child_by_field_name("consequence") {
        Some(c) => c,
        None => return,
    };
    let alternative = match node.child_by_field_name("alternative") {
        Some(a) => a,
        None => return,
    };
    let cons_text = consequence.utf8_text(source).unwrap_or("");
    let alt_text = alternative.utf8_text(source).unwrap_or("");
    if cons_text.starts_with("t(") && alt_text.starts_with("t(") {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Use `t('key', { count })` for pluralization — manual ternaries break CLDR plural rules.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use super::Check;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }
    #[test]
    fn flags_manual_plural() { assert_eq!(run("count === 1 ? t('item') : t('items')").len(), 1); }
    #[test]
    fn allows_t_with_count() { assert!(run("t('item', { count })").is_empty()); }
    #[test]
    fn allows_non_translation_ternary() { assert!(run("count === 1 ? 'item' : 'items'").is_empty()); }
}
```

- [x] **Register all i18n rules + full suite**
```bash
cargo nextest run i18n_
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/i18n_no_manual_pluralization/ src/rules/mod.rs
git commit -m "feat(i18n-no-manual-pluralization): flag count===1 ternary with t() calls"
```

---

## Batch 12 — Security

**Rules (7):** `no-mass-assignment`, `no-open-redirect`, `no-error-details-in-response`, `no-shell-exec`, `no-path-traversal`, `no-unvalidated-url-redirect`, `no-prototype-pollution`

**Note:** All rules are TextCheck registered for TS+TSX+JS. All `Severity::Error`.

**mod.rs pattern (shared by all 7 — only META fields differ):**
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta { /* ... */ };

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

---

### Task 12.1 — `no-mass-assignment`

- [x] Create `src/rules/no_mass_assignment/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-mass-assignment",
    description: "Spreading HTTP body directly into a DB operation enables mass assignment.",
    remediation: "Pick fields explicitly: `db.update(t).set({ name: req.body.name })`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_mass_assignment/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !(t.contains("...req.body") || t.contains("...request.body")) {
                continue;
            }
            if t.contains(".set(") || t.contains(".values(") || t.contains("db.insert(") || t.contains("db.update(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-mass-assignment".into(),
                    message: "Spreading `req.body` into a DB operation allows mass assignment — pick fields explicitly.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_spread_in_values() { assert_eq!(run("db.insert(users).values({ ...req.body })").len(), 1); }
    #[test]
    fn flags_spread_in_set() { assert_eq!(run("db.update(users).set({ ...req.body })").len(), 1); }
    #[test]
    fn allows_explicit_fields() { assert!(run("db.insert(users).values({ name: req.body.name })").is_empty()); }
    #[test]
    fn allows_spread_without_db() { assert!(run("const copy = { ...req.body }").is_empty()); }
}
```

- [x] Register in `src/rules/mod.rs` + run:
```bash
cargo nextest run no_mass_assignment
```

---

### Task 12.2 — `no-open-redirect`

- [x] Create `src/rules/no_open_redirect/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-open-redirect",
    description: "Server-side redirect target from user input enables open redirect.",
    remediation: "Validate the redirect target against an allowlist or ensure it `startsWith('/')`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_open_redirect/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const USER_INPUTS: &[&str] = &[
    "req.query.", "req.params.", "req.body.", "request.query.", "request.params.",
];
const REDIRECT_CALLS: &[&str] = &[
    "res.redirect(", "response.redirect(", "return redirect(", "redirect(",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !REDIRECT_CALLS.iter().any(|r| t.contains(r)) { continue; }
            if USER_INPUTS.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-open-redirect".into(),
                    message: "Redirect target from user input — validate it is a relative path or known origin.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_res_redirect_query() { assert_eq!(run("res.redirect(req.query.next)").len(), 1); }
    #[test]
    fn flags_redirect_params() { assert_eq!(run("return redirect(req.params.url)").len(), 1); }
    #[test]
    fn allows_literal_redirect() { assert!(run("res.redirect('/dashboard')").is_empty()); }
    #[test]
    fn allows_redirect_safe_var() { assert!(run("res.redirect(buildSafeUrl(path))").is_empty()); }
}
```

- [x] Register + run:
```bash
cargo nextest run no_open_redirect
```

---

### Task 12.3 — `no-error-details-in-response`

- [x] Create `src/rules/no_error_details_in_response/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-error-details-in-response",
    description: "Sending `err.message` or `err.stack` in a response leaks internal error details.",
    remediation: "Log the error server-side; return a generic message to the client.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_error_details_in_response/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const ERR_PROPS: &[&str] = &[
    "err.message", "err.stack", "error.message", "error.stack", "e.message", "e.stack",
];
const RESPONSE_CALLS: &[&str] = &[
    "Response.json(", "res.json(", "c.json(", "reply.send(", ".json({",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !ERR_PROPS.iter().any(|e| t.contains(e)) { continue; }
            if RESPONSE_CALLS.iter().any(|r| t.contains(r)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-error-details-in-response".into(),
                    message: "`err.message`/`err.stack` in a response body leaks internal details to clients.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_err_message_in_response_json() {
        assert_eq!(run("return Response.json({ error: err.message }, { status: 500 })").len(), 1);
    }
    #[test]
    fn flags_err_stack_in_res_json() {
        assert_eq!(run("res.json({ error: err.message, stack: err.stack })").len(), 1);
    }
    #[test]
    fn allows_generic_error_message() {
        assert!(run("return Response.json({ error: 'Internal error' }, { status: 500 })").is_empty());
    }
    #[test]
    fn allows_err_message_in_log() {
        assert!(run("logger.error(err.message)").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run no_error_details_in_response
```

---

### Task 12.4 — `no-shell-exec`

- [x] Create `src/rules/no_shell_exec/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-shell-exec",
    description: "`exec()`/`execSync()` with template literal arguments is a command injection vector.",
    remediation: "Use `execFile()` with a separate args array, or `spawn()` with `shell: false`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "node"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_shell_exec/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            // exec/execSync with template literal interpolation
            let exec_with_template = (t.contains("exec(`") || t.contains("execSync(`")) && t.contains("${");
            // shell: true option passed to spawn/spawnSync/execFile
            let shell_true = t.contains("shell: true");
            if exec_with_template || shell_true {
                let message = if shell_true {
                    "`shell: true` enables shell interpretation — pass args as an array with `shell: false`."
                } else {
                    "`exec()` with template literal interpolation is a command injection vector — use `execFile()` with a separate args array."
                };
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-shell-exec".into(),
                    message: message.into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_exec_template_interpolation() { assert_eq!(run("exec(`git clone ${repoUrl}`)").len(), 1); }
    #[test]
    fn flags_exec_sync_template() { assert_eq!(run("execSync(`rm -rf ${dir}`)").len(), 1); }
    #[test]
    fn flags_shell_true() { assert_eq!(run("spawn('cmd', args, { shell: true })").len(), 1); }
    #[test]
    fn allows_exec_plain_literal() { assert!(run("exec('git status')").is_empty()); }
    #[test]
    fn allows_exec_file() { assert!(run("execFile('git', ['clone', repoUrl])").is_empty()); }
    #[test]
    fn allows_template_without_interpolation() { assert!(run("exec(`git status`)").is_empty()); }
}
```

- [x] Register + run:
```bash
cargo nextest run no_shell_exec
```

---

### Task 12.5 — `no-path-traversal`

- [x] Create `src/rules/no_path_traversal/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-path-traversal",
    description: "File system calls with user-supplied path components risk path traversal.",
    remediation: "Wrap the user-supplied segment with `path.basename()` or validate against an allowlist.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security", "node"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_path_traversal/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const FS_CALLS: &[&str] = &[
    "fs.readFile(", "fs.readFileSync(", "fs.writeFile(", "fs.writeFileSync(",
    "fs.appendFile(", "createReadStream(", "createWriteStream(",
];
const USER_SOURCES: &[&str] = &[
    "req.params.", "req.query.", "req.body.", "params.", "query.",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !FS_CALLS.iter().any(|f| t.contains(f)) { continue; }
            // basename() presence indicates the developer sanitized the path
            if t.contains("path.basename(") || t.contains("basename(") { continue; }
            if USER_SOURCES.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-path-traversal".into(),
                    message: "File system call with user-supplied path — use `path.basename()` to prevent directory traversal.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_readfile_with_params() {
        assert_eq!(run("fs.readFileSync(`/uploads/${req.params.file}`)").len(), 1);
    }
    #[test]
    fn flags_readfile_with_query() {
        assert_eq!(run("fs.readFile(req.query.path, 'utf8', cb)").len(), 1);
    }
    #[test]
    fn allows_basename_mitigation() {
        assert!(run("fs.readFileSync(path.basename(req.params.file))").is_empty());
    }
    #[test]
    fn allows_literal_path() {
        assert!(run("fs.readFileSync('/etc/config.json')").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run no_path_traversal
```

---

### Task 12.6 — `no-unvalidated-url-redirect`

- [x] Create `src/rules/no_unvalidated_url_redirect/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-unvalidated-url-redirect",
    description: "Client-side location assignment from user-controlled data enables open redirect.",
    remediation: "Validate the URL against an allowlist or ensure it is a relative path before redirecting.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_unvalidated_url_redirect/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const LOCATION_WRITES: &[&str] = &[
    "window.location =", "window.location.href =", "location.href =",
    "location.replace(", "location.assign(",
];
const USER_SOURCES: &[&str] = &[
    "req.", "params.", "query.", "searchParams.get(", "URLSearchParams",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !LOCATION_WRITES.iter().any(|l| t.contains(l)) { continue; }
            if USER_SOURCES.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-unvalidated-url-redirect".into(),
                    message: "Client-side redirect target from user input — validate the URL before redirecting.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_location_href_from_search_params() {
        assert_eq!(run("window.location.href = searchParams.get('next')").len(), 1);
    }
    #[test]
    fn flags_location_replace_with_query() {
        assert_eq!(run("location.replace(query.redirectUrl)").len(), 1);
    }
    #[test]
    fn allows_literal_location() {
        assert!(run("window.location.href = '/dashboard'").is_empty());
    }
    #[test]
    fn allows_validated_var() {
        assert!(run("window.location.href = safeUrl").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run no_unvalidated_url_redirect
```

---

### Task 12.7 — `no-prototype-pollution`

- [x] Create `src/rules/no_prototype_pollution/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "no-prototype-pollution",
    description: "Deep-merging user-supplied objects can pollute `Object.prototype`.",
    remediation: "Validate/sanitize input before merging, or use a safe merge that rejects `__proto__` keys.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/no_prototype_pollution/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const MERGE_FNS: &[&str] = &[
    "_.merge(", "deepMerge(", "lodash.merge(", "mergeDeep(", "Object.assign(",
];
const USER_DATA: &[&str] = &["req.body", "request.body", "JSON.parse("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !MERGE_FNS.iter().any(|m| t.contains(m)) { continue; }
            if USER_DATA.iter().any(|u| t.contains(u)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-prototype-pollution".into(),
                    message: "Deep-merging user-controlled data risks prototype pollution — sanitize input before merging.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_lodash_merge_req_body() { assert_eq!(run("_.merge(config, req.body)").len(), 1); }
    #[test]
    fn flags_merge_with_json_parse() { assert_eq!(run("deepMerge(defaults, JSON.parse(raw))").len(), 1); }
    #[test]
    fn flags_object_assign_req_body() { assert_eq!(run("Object.assign(target, req.body)").len(), 1); }
    #[test]
    fn allows_merge_safe_data() { assert!(run("_.merge(config, defaults)").is_empty()); }
}
```

- [x] Register all 7 security rules in `src/rules/mod.rs` + run full suite:
```bash
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/no_mass_assignment/ src/rules/no_open_redirect/ src/rules/no_error_details_in_response/ \
        src/rules/no_shell_exec/ src/rules/no_path_traversal/ src/rules/no_unvalidated_url_redirect/ \
        src/rules/no_prototype_pollution/ src/rules/mod.rs
git commit -m "feat(security): add 7 security rules (mass-assignment, open-redirect, error-details, shell-exec, path-traversal, url-redirect, prototype-pollution)"
```

---

## Batch 13 — Better Auth

**Rules (5):** `better-auth-no-disable-csrf`, `better-auth-no-disable-origin-check`, `better-auth-require-rate-limit`, `better-auth-plugin-import-path`, `better-auth-trusted-providers`

**Note:** All rules are TextCheck registered for TS+TSX+JS.

---

### Task 13.1 — `better-auth-no-disable-csrf`

- [x] Create `src/rules/better_auth_no_disable_csrf/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-disable-csrf",
    description: "`disableCSRFCheck: true` removes CSRF protection from Better Auth.",
    remediation: "Remove `disableCSRFCheck` — CSRF protection is enabled by default and must stay on.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/security"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/better_auth_no_disable_csrf/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.trim().contains("disableCSRFCheck: true") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-no-disable-csrf".into(),
                    message: "`disableCSRFCheck: true` disables CSRF protection — remove this option.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_disable_csrf() { assert_eq!(run("betterAuth({ disableCSRFCheck: true })").len(), 1); }
    #[test]
    fn allows_csrf_enabled() { assert!(run("betterAuth({ database: db })").is_empty()); }
}
```

- [x] Register + run:
```bash
cargo nextest run better_auth_no_disable_csrf
```

---

### Task 13.2 — `better-auth-no-disable-origin-check`

- [x] Create `src/rules/better_auth_no_disable_origin_check/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-no-disable-origin-check",
    description: "`disableOriginCheck: true` removes origin validation from Better Auth.",
    remediation: "Remove `disableOriginCheck` — origin validation prevents cross-origin request forgery.",
    severity: Severity::Error,
    doc_url: Some("https://www.better-auth.com/docs/security"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/better_auth_no_disable_origin_check/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.trim().contains("disableOriginCheck: true") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-no-disable-origin-check".into(),
                    message: "`disableOriginCheck: true` removes origin validation — remove this option.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_disable_origin() { assert_eq!(run("betterAuth({ disableOriginCheck: true })").len(), 1); }
    #[test]
    fn allows_trusted_origins() {
        assert!(run("betterAuth({ trustedOrigins: ['https://app.example.com'] })").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run better_auth_no_disable_origin_check
```

---

### Task 13.3 — `better-auth-require-rate-limit`

- [x] Create `src/rules/better_auth_require_rate_limit/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-require-rate-limit",
    description: "Better Auth config without `rateLimit` leaves auth endpoints unprotected.",
    remediation: "Add `rateLimit: { enabled: true }` to your `betterAuth({})` config.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/rate-limiting"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/better_auth_require_rate_limit/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let has_auth = ctx.source.contains("betterAuth(") || ctx.source.contains("createAuth(");
        if !has_auth { return Vec::new(); }
        if ctx.source.contains("rateLimit") { return Vec::new(); }
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("betterAuth(") || line.contains("createAuth(") {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-require-rate-limit".into(),
                    message: "Better Auth config is missing `rateLimit` — add `rateLimit: { enabled: true }` to protect auth endpoints.".into(),
                    severity: Severity::Warning,
                    span: None,
                }];
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_missing_rate_limit() {
        assert_eq!(run("export const auth = betterAuth({ database: db })").len(), 1);
    }
    #[test]
    fn allows_with_rate_limit() {
        assert!(run("export const auth = betterAuth({ rateLimit: { enabled: true } })").is_empty());
    }
    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = doSomething()").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run better_auth_require_rate_limit
```

---

### Task 13.4 — `better-auth-plugin-import-path`

- [x] Create `src/rules/better_auth_plugin_import_path/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-plugin-import-path",
    description: "Importing from `better-auth/plugins` barrel prevents tree-shaking.",
    remediation: "Import from the plugin's specific path: `better-auth/plugins/two-factor`.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/plugins"),
    categories: &["better-auth", "imports"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/better_auth_plugin_import_path/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if !t.starts_with("import") { continue; }
            // Match exact barrel: "better-auth/plugins" (not "better-auth/plugins/something")
            if t.contains("\"better-auth/plugins\"") || t.contains("'better-auth/plugins'") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-plugin-import-path".into(),
                    message: "Import from `better-auth/plugins` barrel prevents tree-shaking — use a specific path like `better-auth/plugins/two-factor`.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_generic_barrel_import() {
        assert_eq!(run("import { twoFactor } from \"better-auth/plugins\"").len(), 1);
    }
    #[test]
    fn flags_single_quote_barrel() {
        assert_eq!(run("import { oAuthProxy } from 'better-auth/plugins'").len(), 1);
    }
    #[test]
    fn allows_specific_plugin_path() {
        assert!(run("import { twoFactor } from \"better-auth/plugins/two-factor\"").is_empty());
    }
    #[test]
    fn allows_core_import() {
        assert!(run("import { betterAuth } from \"better-auth\"").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run better_auth_plugin_import_path
```

---

### Task 13.5 — `better-auth-trusted-providers`

- [x] Create `src/rules/better_auth_trusted_providers/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "better-auth-trusted-providers",
    description: "`accountLinking` enabled without `trustedProviders` allows any OAuth provider to link accounts.",
    remediation: "Add `trustedProviders: ['google', 'github']` to `accountLinking` to restrict which providers may link.",
    severity: Severity::Warning,
    doc_url: Some("https://www.better-auth.com/docs/account-linking"),
    categories: &["security", "better-auth"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/better_auth_trusted_providers/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !ctx.source.contains("accountLinking") { return Vec::new(); }
        if !ctx.source.contains("enabled: true") { return Vec::new(); }
        if ctx.source.contains("trustedProviders") { return Vec::new(); }
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("accountLinking") {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "better-auth-trusted-providers".into(),
                    message: "`accountLinking` is enabled without `trustedProviders` — any OAuth provider can link accounts. Add `trustedProviders` to restrict this.".into(),
                    severity: Severity::Warning,
                    span: None,
                }];
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_linking_without_trusted() {
        assert_eq!(run("betterAuth({ accountLinking: { enabled: true } })").len(), 1);
    }
    #[test]
    fn allows_linking_with_trusted_providers() {
        assert!(run("betterAuth({ accountLinking: { enabled: true, trustedProviders: ['google'] } })").is_empty());
    }
    #[test]
    fn ignores_non_auth_files() {
        assert!(run("const x = 42").is_empty());
    }
}
```

- [x] Register all 5 Better Auth rules in `src/rules/mod.rs` + run full suite:
```bash
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/better_auth_no_disable_csrf/ src/rules/better_auth_no_disable_origin_check/ \
        src/rules/better_auth_require_rate_limit/ src/rules/better_auth_plugin_import_path/ \
        src/rules/better_auth_trusted_providers/ src/rules/mod.rs
git commit -m "feat(better-auth): add 5 Better Auth security rules"
```

---

## Batch 14 — Testing

**Rules (4):** `testing-prefer-msw`, `testing-no-and-in-test-name`, `testing-prefer-test-each`, `testing-no-undefined-mock-var`

**Note:** All rules are TextCheck on TS+TSX+JS. Rules 14.1–14.4 filter by test file path inside `check()` (`.test.` or `.spec.` in filename).

---

### Task 14.1 — `testing-prefer-msw`

- [x] Create `src/rules/testing_prefer_msw/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-msw",
    description: "Mocking HTTP clients directly is brittle — use MSW to intercept at the network layer.",
    remediation: "Replace `vi.mock('axios')` / `global.fetch = vi.fn()` with an MSW request handler.",
    severity: Severity::Warning,
    doc_url: Some("https://mswjs.io/"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/testing_prefer_msw/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const HTTP_CLIENT_MOCKS: &[&str] = &[
    "vi.mock('axios')", "vi.mock(\"axios\")",
    "vi.mock('node-fetch')", "vi.mock(\"node-fetch\")",
    "vi.mock('cross-fetch')", "vi.mock(\"cross-fetch\")",
    "global.fetch = vi.fn()", "globalThis.fetch = vi.fn()",
    "jest.spyOn(global, 'fetch')", "jest.spyOn(globalThis, 'fetch')",
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if HTTP_CLIENT_MOCKS.iter().any(|m| t.contains(m)) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "testing-prefer-msw".into(),
                    message: "Mocking the HTTP client directly is brittle — use MSW to intercept network requests at the handler level.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run_test(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    fn run_src(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.ts"), s))
    }
    #[test]
    fn flags_axios_mock() { assert_eq!(run_test("vi.mock('axios')").len(), 1); }
    #[test]
    fn flags_global_fetch_mock() { assert_eq!(run_test("global.fetch = vi.fn()").len(), 1); }
    #[test]
    fn flags_node_fetch_mock() { assert_eq!(run_test("vi.mock(\"node-fetch\")").len(), 1); }
    #[test]
    fn ignores_non_test_files() { assert!(run_src("vi.mock('axios')").is_empty()); }
    #[test]
    fn allows_msw_handler() { assert!(run_test("server.use(http.get('/api', resolver))").is_empty()); }
}
```

- [x] Register + run:
```bash
cargo nextest run testing_prefer_msw
```

---

### Task 14.2 — `testing-no-and-in-test-name`

- [x] Create `src/rules/testing_no_and_in_test_name/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-and-in-test-name",
    description: "Test names containing \" and \" usually test multiple behaviors — split into separate tests.",
    remediation: "Write one test per behavior; use `describe` to group related cases.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/testing_no_and_in_test_name/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            // Match: test('... and ...', or it('... and ...', — detect " and " inside the test name string
            let is_test_call = t.starts_with("test(") || t.starts_with("it(")
                || t.contains("  test(") || t.contains("  it(");
            if !is_test_call { continue; }
            // Check if the string argument (between first quote pair) contains " and "
            if let Some(name) = extract_test_name(t) {
                if name.contains(" and ") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "testing-no-and-in-test-name".into(),
                        message: format!("Test name {:?} contains \" and \" — split into two focused tests.", name),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

fn extract_test_name(line: &str) -> Option<String> {
    for prefix in &["test('", "it('", "test(\"", "it(\""] {
        if let Some(pos) = line.find(prefix) {
            let rest = &line[pos + prefix.len()..];
            let close = if prefix.ends_with('\'') { '\'' } else { '"' };
            if let Some(end) = rest.find(close) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    #[test]
    fn flags_and_in_name() {
        assert_eq!(run("test('validates email and sends confirmation', () => {})").len(), 1);
    }
    #[test]
    fn flags_it_with_and() {
        assert_eq!(run("it('creates user and returns token', () => {})").len(), 1);
    }
    #[test]
    fn allows_single_behavior() {
        assert!(run("test('validates email format', () => {})").is_empty());
    }
    #[test]
    fn allows_and_in_describe() {
        // describe names can legitimately contain "and"
        assert!(run("describe('login and registration', () => {})").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run testing_no_and_in_test_name
```

---

### Task 14.3 — `testing-prefer-test-each`

- [x] Create `src/rules/testing_prefer_test_each/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-prefer-test-each",
    description: "3+ tests with a common name prefix can be collapsed into a single `test.each` table.",
    remediation: "Use `test.each([...])` to express parameterized cases without repeating test boilerplate.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/testing_prefer_test_each/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::collections::HashSet;

#[derive(Debug)]
pub struct Check;

fn extract_test_name(line: &str) -> Option<String> {
    for prefix in &["test('", "it('", "test(\"", "it(\""] {
        if let Some(pos) = line.find(prefix) {
            let rest = &line[pos + prefix.len()..];
            let close = if prefix.ends_with('\'') { '\'' } else { '"' };
            if let Some(end) = rest.find(close) {
                return Some(rest[..end].to_lowercase());
            }
        }
    }
    None
}

fn common_prefix(a: &str, b: &str) -> String {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).map(|(c, _)| c).collect()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        let mut test_names: Vec<(usize, String)> = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(name) = extract_test_name(line) {
                test_names.push((idx + 1, name));
            }
        }
        if test_names.len() < 3 { return Vec::new(); }

        let mut flagged: HashSet<usize> = HashSet::new();
        let mut diagnostics = Vec::new();
        let n = test_names.len();
        for i in 0..n {
            if flagged.contains(&i) { continue; }
            for j in i + 1..n {
                let prefix = common_prefix(&test_names[i].1, &test_names[j].1);
                if prefix.len() < 10 { continue; }
                // Look for a third test sharing the same prefix
                for k in j + 1..n {
                    if test_names[k].1.starts_with(&prefix) && !flagged.contains(&i) {
                        flagged.insert(i);
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: test_names[i].0,
                            column: 1,
                            rule_id: "testing-prefer-test-each".into(),
                            message: format!(
                                "3+ tests share the prefix {:?} — use `test.each` to express these as a data-driven table.",
                                prefix.trim()
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    #[test]
    fn flags_three_tests_with_common_prefix() {
        let src = [
            "test('returns 200 for valid input', () => {})",
            "test('returns 400 for missing field', () => {})",
            "test('returns 422 for invalid format', () => {})",
        ].join("\n");
        assert!(!run(&src).is_empty());
    }
    #[test]
    fn allows_two_tests_only() {
        let src = [
            "test('returns 200 for valid', () => {})",
            "test('returns 400 for invalid', () => {})",
        ].join("\n");
        assert!(run(&src).is_empty());
    }
    #[test]
    fn allows_tests_with_short_or_no_common_prefix() {
        let src = [
            "test('creates a user', () => {})",
            "test('deletes a post', () => {})",
            "test('updates a comment', () => {})",
        ].join("\n");
        assert!(run(&src).is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run testing_prefer_test_each
```

---

### Task 14.4 — `testing-no-undefined-mock-var`

**Heuristic:** collect module-level `let` var names (zero-indented lines starting with `let `), find `vi.mock(` factory blocks, flag if any collected name appears in the factory body. Variables inside `vi.hoisted(` are exempt.

- [x] Create `src/rules/testing_no_undefined_mock_var/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "testing-no-undefined-mock-var",
    description: "`vi.mock()` factories are hoisted — module-level `let` vars they reference will be `undefined`.",
    remediation: "Declare the variable inside `vi.hoisted()` so it is initialized before the factory runs.",
    severity: Severity::Error,
    doc_url: Some("https://vitest.dev/api/vi#vi-hoisted"),
    categories: &["testing"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/testing_no_undefined_mock_var/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return Vec::new();
        }
        if !ctx.source.contains("vi.mock(") { return Vec::new(); }

        // Phase 1: collect module-level `let` var names (zero-indented, outside vi.hoisted)
        let mut module_lets: Vec<String> = Vec::new();
        let mut in_hoisted = false;
        let mut hoisted_depth: usize = 0;
        for line in ctx.source.lines() {
            let t = line.trim();
            if t.contains("vi.hoisted(") {
                in_hoisted = true;
            }
            if in_hoisted {
                hoisted_depth = hoisted_depth
                    .saturating_add(t.matches('(').count())
                    .saturating_sub(t.matches(')').count());
                if hoisted_depth == 0 { in_hoisted = false; }
                continue;
            }
            // Zero-indented `let` = module-level
            if line.starts_with("let ") {
                let rest = &line[4..];
                let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() { module_lets.push(name); }
            }
        }
        if module_lets.is_empty() { return Vec::new(); }

        // Phase 2: collect vi.mock factory bodies and check for references
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut diagnostics = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if line.trim().contains("vi.mock(") {
                let mock_line = i + 1;
                // Collect factory body by tracking parenthesis depth
                let mut body = String::new();
                let mut depth: usize = 0;
                let mut j = i;
                while j < lines.len() {
                    let l = lines[j];
                    depth = depth
                        .saturating_add(l.matches('(').count())
                        .saturating_sub(l.matches(')').count());
                    body.push_str(l);
                    body.push('\n');
                    if depth == 0 && j > i { break; }
                    j += 1;
                }
                for var_name in &module_lets {
                    if body.contains(var_name.as_str()) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: mock_line,
                            column: 1,
                            rule_id: "testing-no-undefined-mock-var".into(),
                            message: format!(
                                "`{}` is declared at module level and referenced in a `vi.mock()` factory — it will be `undefined` due to hoisting. Declare it inside `vi.hoisted()` instead.",
                                var_name
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        break;
                    }
                }
                i = j;
            }
            i += 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), s))
    }
    #[test]
    fn flags_module_let_in_mock_factory() {
        let src = r#"
let mockFn = vi.fn()
vi.mock('module', () => ({ default: mockFn }))
"#;
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_var_inside_hoisted() {
        let src = r#"
const mockFn = vi.hoisted(() => vi.fn())
vi.mock('module', () => ({ default: mockFn }))
"#;
        assert!(run(src).is_empty());
    }
    #[test]
    fn allows_mock_without_module_lets() {
        let src = r#"
vi.mock('module', () => ({ default: vi.fn() }))
"#;
        assert!(run(src).is_empty());
    }
    #[test]
    fn ignores_non_test_files() {
        let src = "let x = 1\nvi.mock('m', () => ({ a: x }))";
        let d = Check.check(&CheckCtx::for_test(Path::new("foo.ts"), src));
        assert!(d.is_empty());
    }
}
```

- [x] Register all 4 testing rules in `src/rules/mod.rs` + run full suite:
```bash
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/testing_prefer_msw/ src/rules/testing_no_and_in_test_name/ \
        src/rules/testing_prefer_test_each/ src/rules/testing_no_undefined_mock_var/ \
        src/rules/mod.rs
git commit -m "feat(testing): add 4 testing rules (prefer-msw, no-and-in-name, prefer-test-each, no-undefined-mock-var)"
```

---

## Batch 15 — Drizzle ORM

**Rules (4):** `drizzle-returning-on-insert-update`, `drizzle-no-sql-raw-with-variable`, `drizzle-no-select-without-limit`, `drizzle-zod-prefer-generated-schema`

**Note:** All rules are TextCheck registered for TS+TSX+JS. Rules 15.1 and 15.3 do multi-line chain analysis.

---

### Task 15.1 — `drizzle-returning-on-insert-update`

- [x] Create `src/rules/drizzle_returning_on_insert_update/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-returning-on-insert-update",
    description: "Drizzle insert/update without `.returning()` wastes a round-trip on a follow-up SELECT.",
    remediation: "Chain `.returning()` to get the inserted/updated row in a single query.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/insert#insert-returning"),
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/drizzle_returning_on_insert_update/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.contains("db.insert(") || t.contains("db.update(") {
                let start_line = i + 1;
                // Collect the full method chain (up to 12 lines or first semicolon)
                let mut chain = String::new();
                let mut depth: usize = 0;
                let mut j = i;
                while j < lines.len() && j - i <= 12 {
                    let l = lines[j];
                    depth = depth
                        .saturating_add(l.matches('(').count())
                        .saturating_sub(l.matches(')').count());
                    chain.push_str(l);
                    chain.push('\n');
                    if l.trim().ends_with(';') || (depth == 0 && j > i) { break; }
                    j += 1;
                }
                let is_mutation = chain.contains(".values(") || chain.contains(".set(");
                if is_mutation && !chain.contains(".returning(") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start_line,
                        column: 1,
                        rule_id: "drizzle-returning-on-insert-update".into(),
                        message: "Drizzle insert/update without `.returning()` — chain `.returning()` to get the result without a follow-up SELECT.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                i = j;
            }
            i += 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_insert_without_returning() {
        assert_eq!(run("await db.insert(users).values({ name: 'Alice' })").len(), 1);
    }
    #[test]
    fn flags_update_without_returning() {
        assert_eq!(run("await db.update(users).set({ active: false }).where(eq(users.id, id))").len(), 1);
    }
    #[test]
    fn allows_insert_with_returning() {
        assert!(run("const [u] = await db.insert(users).values({ name: 'Alice' }).returning()").is_empty());
    }
    #[test]
    fn allows_update_with_returning() {
        assert!(run("await db.update(users).set({ active: false }).where(eq(users.id, id)).returning()").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run drizzle_returning_on_insert_update
```

---

### Task 15.2 — `drizzle-no-sql-raw-with-variable`

- [x] Create `src/rules/drizzle_no_sql_raw_with_variable/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-sql-raw-with-variable",
    description: "`sql.raw()` with a non-literal argument is a SQL injection vector.",
    remediation: "Use `sql` tagged template literals with parameterized interpolation, or `sql.identifier()` for identifiers.",
    severity: Severity::Error,
    doc_url: Some("https://orm.drizzle.team/docs/sql#sqlraw"),
    categories: &["drizzle", "security"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/drizzle_no_sql_raw_with_variable/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let t = line.trim();
            if let Some(pos) = t.find("sql.raw(") {
                let after_paren = &t[pos + 8..];
                // Allow only plain string literals (starts with " or ')
                let is_string_literal = after_paren.starts_with('"') || after_paren.starts_with('\'');
                if !is_string_literal {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "drizzle-no-sql-raw-with-variable".into(),
                        message: "`sql.raw()` with a non-literal argument is a SQL injection vector — use `sql` tagged templates with parameterized values instead.".into(),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_variable_argument() { assert_eq!(run("sql.raw(userInput)").len(), 1); }
    #[test]
    fn flags_template_literal() {
        assert_eq!(run("sql.raw(`SELECT * FROM ${tableName}`)").len(), 1);
    }
    #[test]
    fn allows_string_literal_double_quote() {
        assert!(run("sql.raw(\"SELECT 1\")").is_empty());
    }
    #[test]
    fn allows_string_literal_single_quote() {
        assert!(run("sql.raw('NOW()')").is_empty());
    }
    #[test]
    fn allows_tagged_template() {
        assert!(run("sql`WHERE id = ${userId}`").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run drizzle_no_sql_raw_with_variable
```

---

### Task 15.3 — `drizzle-no-select-without-limit`

- [x] Create `src/rules/drizzle_no_select_without_limit/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-no-select-without-limit",
    description: "`db.select().from(table)` without `.limit()` or `.where()` scans the entire table.",
    remediation: "Add `.limit(n)` or `.where(condition)` to bound the result set.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/select#basic-and-partial-select"),
    categories: &["drizzle"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/drizzle_no_select_without_limit/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.contains("db.select(") && t.contains(".from(") {
                let start_line = i + 1;
                // Collect the full chain
                let mut chain = String::new();
                let mut depth: usize = 0;
                let mut j = i;
                while j < lines.len() && j - i <= 10 {
                    let l = lines[j];
                    depth = depth
                        .saturating_add(l.matches('(').count())
                        .saturating_sub(l.matches(')').count());
                    chain.push_str(l);
                    chain.push('\n');
                    if l.trim().ends_with(';') || (depth == 0 && j > i) { break; }
                    j += 1;
                }
                if !chain.contains(".limit(") && !chain.contains(".where(") {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: start_line,
                        column: 1,
                        rule_id: "drizzle-no-select-without-limit".into(),
                        message: "`db.select().from(table)` without `.limit()` or `.where()` scans the entire table — add a limit or filter.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                i = j;
            }
            i += 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_unbounded_select() {
        assert_eq!(run("const users = await db.select().from(usersTable)").len(), 1);
    }
    #[test]
    fn flags_partial_select_without_limit() {
        assert_eq!(run("const all = await db.select({ id: users.id }).from(usersTable)").len(), 1);
    }
    #[test]
    fn allows_select_with_where() {
        assert!(run("await db.select().from(usersTable).where(eq(usersTable.active, true))").is_empty());
    }
    #[test]
    fn allows_select_with_limit() {
        assert!(run("await db.select().from(usersTable).limit(20)").is_empty());
    }
}
```

- [x] Register + run:
```bash
cargo nextest run drizzle_no_select_without_limit
```

---

### Task 15.4 — `drizzle-zod-prefer-generated-schema`

**Heuristic:** file imports both `drizzle-orm` and `zod`, defines a table with `pgTable(`/`mysqlTable(`/`sqliteTable(`, and also contains a manual `z.object({` — flag the `z.object` as likely duplicating the table schema.

- [x] Create `src/rules/drizzle_zod_prefer_generated_schema/mod.rs`:
```rust
mod text;
use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::files::Language;

pub const META: RuleMeta = RuleMeta {
    id: "drizzle-zod-prefer-generated-schema",
    description: "Manual `z.object({})` in a Drizzle schema file duplicates column definitions.",
    remediation: "Use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` to generate Zod schemas from the table definition.",
    severity: Severity::Warning,
    doc_url: Some("https://orm.drizzle.team/docs/zod"),
    categories: &["drizzle", "zod"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
        ],
    }
}
```

- [x] Create `src/rules/drizzle_zod_prefer_generated_schema/text.rs`:
```rust
use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const TABLE_DEFS: &[&str] = &["pgTable(", "mysqlTable(", "sqliteTable(", "table("];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only fire when all three conditions are true (file-level check)
        let has_drizzle = ctx.source.contains("drizzle-orm") || ctx.source.contains("drizzle-zod");
        let has_zod = ctx.source.contains("from 'zod'") || ctx.source.contains("from \"zod\"");
        let has_table = TABLE_DEFS.iter().any(|t| ctx.source.contains(t));
        // If already using drizzle-zod generators, allow manual z.object elsewhere
        let uses_generator = ctx.source.contains("createInsertSchema") || ctx.source.contains("createSelectSchema");

        if !has_drizzle || !has_zod || !has_table || uses_generator {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.trim().contains("z.object(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "drizzle-zod-prefer-generated-schema".into(),
                    message: "Manual `z.object({})` in a Drizzle schema file likely duplicates column definitions — use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }
    #[test]
    fn flags_manual_zod_in_drizzle_file() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core'
import { z } from 'zod'
export const users = pgTable('users', { name: text('name') })
export const insertUserSchema = z.object({ name: z.string() })
"#;
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_generated_schema() {
        let src = r#"
import { pgTable, text } from 'drizzle-orm/pg-core'
import { createInsertSchema } from 'drizzle-zod'
export const users = pgTable('users', { name: text('name') })
export const insertUserSchema = createInsertSchema(users)
"#;
        assert!(run(src).is_empty());
    }
    #[test]
    fn ignores_non_drizzle_zod_files() {
        let src = r#"
import { z } from 'zod'
export const schema = z.object({ name: z.string() })
"#;
        assert!(run(src).is_empty());
    }
}
```

- [x] Register all 4 Drizzle rules in `src/rules/mod.rs` + run full suite:
```bash
cargo nextest run && cargo clippy --all --all-targets -- -D warnings
git add src/rules/drizzle_returning_on_insert_update/ src/rules/drizzle_no_sql_raw_with_variable/ \
        src/rules/drizzle_no_select_without_limit/ src/rules/drizzle_zod_prefer_generated_schema/ \
        src/rules/mod.rs
git commit -m "feat(drizzle): add 4 Drizzle ORM rules (returning, no-sql-raw, no-select-without-limit, prefer-generated-schema)"
```

---

## Final verification after all batches

- [x] `cargo nextest run` — all tests pass
- [x] `cargo clippy --all --all-targets -- -D warnings` — zero warnings
- [x] `./target/release/comply src/` — comply finds no violations in its own source
- [x] Update `RULES_TO_ADD.md` — mark all implemented rules as done
- [x] One consolidated PR per batch or grouped PR for trivial batches (7 v5 rename rules → single PR)
