//! try-catch-new-url backend — flag `new URL(...)` not wrapped in a try.
//!
//! Detection: every `new_expression` whose constructor is `URL`, not
//! enclosed by a `try_statement` body within the same function boundary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

fn is_inside_try_body(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "try_statement"
            && let Some(body) = n.child_by_field_name("body")
        {
            let ns = node.start_byte();
            let ne = node.end_byte();
            if ns >= body.start_byte() && ne <= body.end_byte() {
                return true;
            }
        }
        if FUNCTION_KINDS.contains(&n.kind()) {
            return false;
        }
        current = n.parent();
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "new_expression" {
                return;
            }
            let Some(ctor) = node.child_by_field_name("constructor") else { return };
            let Ok(ctor_name) = ctor.utf8_text(source) else { return };
            if ctor_name != "URL" {
                return;
            }
            if is_inside_try_body(node) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "try-catch-new-url".into(),
                message: "`new URL(...)` throws on invalid input — wrap in try/catch \
                          or gate with `URL.canParse(s)` first."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bare_new_url() {
        let d = run_on("const u = new URL(input);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "try-catch-new-url");
    }

    #[test]
    fn flags_new_url_in_fn() {
        let d = run_on("function f(s) { return new URL(s); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inside_try() {
        assert!(run_on("try { const u = new URL(input); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn allows_other_constructors() {
        assert!(run_on("const u = new MyUrl(input);").is_empty());
    }
}
