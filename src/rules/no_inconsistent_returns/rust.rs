//! no-inconsistent-returns Rust backend.
//!
//! Flag functions/closures that mix `return expr;` with bare `return;`.
//!
//! Walks tree-sitter AST nodes (`function_item`, `closure_expression`) and
//! collects only their direct `return_expression` children — nested function,
//! closure, and `async` block subtrees are skipped so an inner scope's
//! `return` is not attributed to the parent function.
//!
//! A `return_expression` with at least one named child returns a value;
//! otherwise it is bare.
//!
//! A `function_item` that returns unit (`-> ()` or no `return_type`) is exempt:
//! its `return <expr>;` is always a unit-typed early return, so the value/bare
//! mix is not an inconsistency.

use crate::diagnostic::{Diagnostic, Severity};

/// Recursively scan `node`'s subtree for `return_expression` nodes,
/// stopping at nested function, closure, and `async` block boundaries so
/// inner returns are attributed to the inner scope only.
fn collect_returns<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        match child.kind() {
            "function_item" | "closure_expression" | "async_block" => {
                // Skip — an inner fn, closure, or `async`/`async move` block
                // is its own return scope; its returns belong to it, not here.
            }
            "return_expression" => {
                out.push(child);
                // Don't descend further: a value expression inside the
                // return cannot itself be a top-level return for this fn.
            }
            _ => collect_returns(child, out),
        }
    }
}

/// True if a `return_expression` carries a value (i.e. has a named child).
fn return_has_value(ret: tree_sitter::Node) -> bool {
    ret.named_child_count() > 0
}

/// True when `node` is a `function_item` that returns unit `()` — either it has
/// no `return_type` field (implicit `()`) or an explicit unit type (`-> ()`).
/// In such a function the compiler forbids `return non_unit;`, so every
/// `return <expr>;` is a unit-typed early return, semantically identical to a
/// bare `return;`. The value-vs-bare distinction is then spurious and the
/// inconsistency check is skipped. Closures are excluded — they rarely annotate
/// a return type, so the same structural signal is unavailable.
fn returns_unit(node: tree_sitter::Node) -> bool {
    if node.kind() != "function_item" {
        return false;
    }
    match node.child_by_field_name("return_type") {
        None => true,
        Some(ty) => ty.kind() == "unit_type",
    }
}

crate::ast_check! { on ["function_item", "closure_expression"] => |node, _source, ctx, diagnostics|
    // Body location: function_item has a "body" field (block);
    // closure_expression has a "body" field (block or expression).
    let Some(body) = node.child_by_field_name("body") else { return };

    // A unit-returning `function_item` (`-> ()` or no return type) can only hold
    // unit-typed `return <expr>;` — the compiler rejects any other value there —
    // so a value/bare mix carries no inconsistency.
    if returns_unit(node) {
        return;
    }

    let mut returns: Vec<tree_sitter::Node> = Vec::new();
    collect_returns(body, &mut returns);

    let mut has_value = false;
    let mut has_bare = false;
    for ret in &returns {
        if return_has_value(*ret) {
            has_value = true;
        } else {
            has_bare = true;
        }
    }

    if has_value && has_bare {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-inconsistent-returns".into(),
            message: "Function has inconsistent returns \u{2014} some paths return a value, others return nothing.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_mixed_returns() {
        let code = r#"
fn foo(x: bool) -> Option<i32> {
    if x {
        return 42;
    }
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_consistent_value_returns() {
        let code = r#"
fn foo(x: bool) -> i32 {
    if x {
        return 42;
    }
    return 0;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_consistent_bare_returns() {
        let code = r#"
fn foo(x: bool) {
    if x {
        return;
    }
    return;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn does_not_attribute_closure_returns_to_outer_fn() {
        // Outer fn has only `return 1;`. Inner closure has `return;`.
        // Without the AST walk this was flagged as inconsistent.
        let code = r#"
fn outer() -> i32 {
    let _ = |x: i32| {
        if x == 0 {
            return;
        }
        println!("{x}");
    };
    return 1;
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_closure_with_inconsistent_returns() {
        let code = r#"
fn outer() {
    let _ = |x: i32| {
        if x == 0 {
            return;
        }
        return x + 1;
    };
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn allows_unit_fn_with_tail_call_return() {
        // Regression for issue #7224 (pola-rs/polars `zip_outer_validity`): a fn
        // with no declared return type returns `()`; the tail-recursive
        // `return self.zip(other);` is a unit-typed early return, not a value.
        let code = r#"
pub fn zip(&mut self, other: &S) {
    if a {
        return;
    }
    if b {
        self.rechunk_mut();
        return self.zip(other);
    }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_explicit_unit_return_type() {
        // Explicit `-> ()` is likewise unit-returning: `return bar();` where
        // `bar() -> ()` is a unit early return, mixed with bare `return;`.
        let code = r#"
fn foo(a: bool) -> () {
    if a {
        return;
    }
    return bar();
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn still_flags_non_unit_return_type_with_bare_return() {
        // An explicit non-unit return type (`-> Option<i32>`) mixing a value
        // return with a bare `return;` is a genuine inconsistency: still flags.
        let code = r#"
fn foo(a: bool) -> Option<i32> {
    if a {
        return Some(1);
    }
    return;
}
"#;
        assert_eq!(run_on(code).len(), 1);
    }

    #[test]
    fn does_not_attribute_async_block_returns_to_outer_fn() {
        // `outer` returns a value on every path (42). The value-vs-bare mix
        // comes entirely from distinct spawned `async` futures, each its own
        // return scope — so `outer` must not be flagged.
        let code = r#"
pub async fn outer() -> u32 {
    tokio::spawn(async move {
        let r: Result<(), ()> = async {
            if cond() { return Ok(()); }
            Ok(())
        }.await;
        let _ = r;
    });

    tokio::spawn(async move {
        if cond() { return; }
        do_work();
    });

    42
}
"#;
        assert!(run_on(code).is_empty());
    }
}
