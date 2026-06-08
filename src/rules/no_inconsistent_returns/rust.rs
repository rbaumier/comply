//! no-inconsistent-returns Rust backend.
//!
//! Flag functions/closures that mix `return expr;` with bare `return;`.
//!
//! Walks tree-sitter AST nodes (`function_item`, `closure_expression`) and
//! collects only their direct `return_expression` children — nested function
//! and closure subtrees are skipped so an inner closure's `return` is not
//! attributed to the parent function.
//!
//! A `return_expression` with at least one named child returns a value;
//! otherwise it is bare.

use crate::diagnostic::{Diagnostic, Severity};

/// Recursively scan `node`'s subtree for `return_expression` nodes,
/// stopping at nested function/closure boundaries so inner returns
/// are attributed to the inner function only.
fn collect_returns<'t>(node: tree_sitter::Node<'t>, out: &mut Vec<tree_sitter::Node<'t>>) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        match child.kind() {
            "function_item" | "closure_expression" => {
                // Skip — its returns belong to that inner function.
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

crate::ast_check! { on ["function_item", "closure_expression"] => |node, _source, ctx, diagnostics|
    // Body location: function_item has a "body" field (block);
    // closure_expression has a "body" field (block or expression).
    let Some(body) = node.child_by_field_name("body") else { return };

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
            severity: Severity::Warning,
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
}
