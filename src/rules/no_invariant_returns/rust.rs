//! no-invariant-returns Rust backend.
//!
//! Walks `function_item` nodes, collects the `return_expression` nodes
//! belonging directly to the function body plus the function block's tail
//! expression (Rust's implicit return), and flags the function only when every
//! return site is provably the same literal. A return site whose value is not a
//! literal (a computed expression, or a control-flow tail such as `if`/`match`)
//! makes invariance unprovable, so the function is left unflagged.
//!
//! Nested `function_item` and `closure_expression` subtrees are skipped so
//! an inner closure's `return` is not attributed to the outer function.

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
            }
            _ => collect_returns(child, out),
        }
    }
}

/// Extract a normalized literal text from a `return_expression` value, or
/// from a tail expression. Returns `None` for non-literals.
fn literal_text<'a>(value: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let kind = value.kind();
    let text = value.utf8_text(source).ok()?.trim();
    match kind {
        "integer_literal" | "float_literal" | "string_literal" | "char_literal"
        | "boolean_literal" | "raw_string_literal" => Some(text),
        // `None` shows up as a regular identifier in expression position.
        "identifier" if text == "None" => Some(text),
        _ => None,
    }
}

/// Pull the value of a `return_expression`, if any (bare `return` has no
/// child). Returns `None` for bare returns and non-literal values.
fn return_value_literal<'a>(ret: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let value = ret.named_child(0)?;
    literal_text(value, source)
}

/// Return the block's trailing expression (Rust's implicit return), if any.
///
/// Bare-value tails (literals, identifiers like `None`) appear directly as the
/// last named child. Block-like tails (`if`, `match`, `loop`, …) are wrapped in
/// an `expression_statement` that — unlike a real statement — carries no
/// trailing `;`; that wrapper is unwrapped to expose the actual tail
/// expression. Statements (`let_declaration`, or an `expression_statement`
/// terminated by `;`) are not tails and yield `None`.
fn block_tail_expression<'t>(block: tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    if block.kind() != "block" {
        return None;
    }
    let last = block.named_child(block.named_child_count().checked_sub(1)?)?;
    match last.kind() {
        "let_declaration" => None,
        "expression_statement" => {
            // A trailing `;` makes this a statement, not an implicit return.
            let has_semicolon = last.child(last.child_count().checked_sub(1)?)?.kind() == ";";
            if has_semicolon {
                None
            } else {
                last.named_child(0)
            }
        }
        _ => Some(last),
    }
}

crate::ast_check! { on ["function_item"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };

    let mut returns: Vec<tree_sitter::Node> = Vec::new();
    collect_returns(body, &mut returns);

    let mut literals: Vec<&str> = Vec::new();
    for ret in &returns {
        let Some(lit) = return_value_literal(*ret, source) else {
            return; // Non-literal return — can't prove invariance.
        };
        literals.push(lit);
    }

    if let Some(tail) = block_tail_expression(body) {
        let Some(lit) = literal_text(tail, source) else {
            // Tail is a non-literal expression — bail out unless there are
            // no return statements at all (in which case we have nothing).
            return;
        };
        literals.push(lit);
    }

    if literals.len() < 2 {
        return;
    }

    let first = literals[0];
    if !literals.iter().all(|l| *l == first) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-invariant-returns".into(),
        message: "Function always returns the same literal value \u{2014} consider using a constant instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_invariant_true() {
        let src = r#"
fn is_enabled(x: i32) -> bool {
    if x > 0 {
        return true;
    }
    return true;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_returns() {
        let src = r#"
fn is_positive(n: i32) -> bool {
    if n > 0 {
        return true;
    }
    return false;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_single_return() {
        let src = r#"
fn get_value() -> i32 {
    return 42;
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #1466 — guard `return None` early-exits plus a non-literal
    // `Some(computed)` happy path in an `if/else` tail must not be flagged.
    #[test]
    fn allows_guard_none_with_some_if_else_tail() {
        let src = r#"
fn literal(&self) -> Option<String> {
    if self.opts.case_insensitive {
        return None;
    }
    let mut lit = String::new();
    for t in &*self.tokens {
        let Token::Literal(c) = *t else { return None };
        lit.push(c);
    }
    if lit.is_empty() { None } else { Some(lit) }
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Issue #1466 — guard `return None` plus a `Some(computed)` happy path in
    // a `match` tail must not be flagged.
    #[test]
    fn allows_guard_none_with_some_match_tail() {
        let src = r#"
fn open(&self, file: &File) -> Option<Mmap> {
    if !self.is_enabled() {
        return None;
    }
    if cfg!(target_os = "macos") {
        return None;
    }
    match unsafe { Mmap::map(file) } {
        Ok(mmap) => Some(mmap),
        Err(_) => None,
    }
}
"#;
        assert!(run_on(src).is_empty());
    }

    // Negative space: a genuinely invariant function whose explicit guard
    // returns and bare implicit tail all yield the same literal must still fire.
    #[test]
    fn flags_invariant_none_across_returns_and_tail() {
        let src = r#"
fn always_none(x: i32) -> Option<i32> {
    if x > 0 {
        return None;
    }
    do_side_effect();
    None
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
