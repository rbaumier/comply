//! array-callback-without-return Rust backend.
//!
//! Flag iterator method closures with block body but no return/expression.
//! In Rust: `.map(|x| { ... })` with block body missing a trailing expression.

use crate::diagnostic::{Diagnostic, Severity};

const ITERATOR_METHODS: &[&str] = &[
    "map",
    "filter",
    "find",
    "any",
    "all",
    "flat_map",
    "filter_map",
];

fn is_iterator_method_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "field_expression" {
        return false;
    }
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    let name = field.utf8_text(source).unwrap_or("");
    ITERATOR_METHODS.contains(&name)
}

/// True when the closure body block produces a value: it ends in a tail
/// expression or contains an explicit `return`.
///
/// In Rust a block returns its final expression when that expression has no
/// trailing `;`. tree-sitter-rust wraps block-like tail expressions (`match`,
/// `if`/`else`, `loop`, `while`, `unsafe`) in an `expression_statement`; such a
/// node is the implicit return iff it does not end in a semicolon. Other tail
/// expressions (`x + 1`, `async { .. }`) appear directly as the block's final
/// named child.
fn block_returns_value(block: tree_sitter::Node) -> bool {
    let count = block.named_child_count();
    let Some(last) = count.checked_sub(1).and_then(|i| block.named_child(i)) else {
        return false;
    };
    match last.kind() {
        "let_declaration" | "empty_statement" => false,
        "expression_statement" => !expression_statement_has_semicolon(last),
        _ => true,
    }
}

/// True when an `expression_statement` ends in a `;` (a discarded statement,
/// not a tail expression).
fn expression_statement_has_semicolon(stmt: tree_sitter::Node) -> bool {
    stmt.child(stmt.child_count().saturating_sub(1))
        .is_some_and(|last| last.kind() == ";")
}

fn has_return(node: tree_sitter::Node) -> bool {
    if node.kind() == "return_expression" {
        return true;
    }
    if matches!(node.kind(), "closure_expression" | "function_item") {
        return false;
    }
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .any(has_return)
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_iterator_method_call(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(callback) = args.named_child(0) else { return };

    if callback.kind() != "closure_expression" {
        return;
    }
    let Some(body) = callback.child_by_field_name("body") else { return };
    if body.kind() != "block" {
        return;
    }

    if !block_returns_value(body) && !has_return(body) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "array-callback-without-return".into(),
            message: "Iterator callback with block body but no return value.".into(),
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
    fn flags_map_without_return() {
        let src = "fn f() { vec![1].iter().map(|x| { let y = x; }); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_map_with_tail_expr() {
        let src = "fn f() { vec![1].iter().map(|x| { x + 1 }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_concise_closure() {
        let src = "fn f() { vec![1].iter().map(|x| x + 1); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_block_as_return() {
        // async { ... } is an expression that IS the return value
        let src = "fn f() { vec![1].iter().map(|x| { async { let y = x; } }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_async_block_with_statements() {
        // async { ... } with inner statements is still an expression return
        let src = "fn f() { vec![1].iter().map(|x| { async { let y = x; if y > 0 {} } }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_block_with_only_let_before_async() {
        // Has let statement but then an async block - wait, async block IS the return
        // Actually this should NOT flag because the block ends with the async expression
        let src = "fn f() { vec![1].iter().map(|x| { let _z = 0; async { } }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_trailing_match_as_implicit_return() {
        // Issue #1503: a trailing `match` with no `;` is the tail expression.
        let src = "fn f() { (0..n).map(|i| { let field = g(i); match h(field) { Ok(o) => Ok(Some(o)), Err(e) => Err(e), } }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_trailing_if_else_as_implicit_return() {
        // Issue #1503: a trailing `if`/`else` with no `;` is the tail expression.
        let src = "fn f() { v.iter().map(|x| { let y = x; if y > 0 { y } else { -y } }); }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_block_ending_in_semicolon_statement() {
        // Negative space: a `;`-terminated final statement produces no value.
        let src = "fn f() { vec![1].iter().map(|x| { foo(x); }); }";
        assert_eq!(run_on(src).len(), 1);
    }
}
