//! no-nullish-default-on-input backend — reject `x ?? default` / `x || fallback`
//! on function parameters.
//!
//! Why: using `x ?? 0` or `x || []` on an external input silently paves
//! over invalid values. If the caller passes garbage, the function happily
//! runs with `0` or `[]` and the bug surfaces far from where it was
//! introduced. The correct response is to validate at the boundary and
//! reject the call with a Result error.
//!
//! Detection: walk `binary_expression` nodes whose operator is `??` or
//! `||` and whose left operand is an identifier matching a function
//! parameter name in scope. Cheap heuristic: collect parameter names from
//! every enclosing function node, then flag any `param ?? x` / `param || x`
//! pattern inside that function body.

use std::collections::HashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let params = collect_all_parameters(tree, source_bytes);
        if params.is_empty() {
            return vec![];
        }
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "binary_expression" {
                return;
            }
            let Some(op) = node.child_by_field_name("operator") else {
                return;
            };
            let Ok(op_text) = op.utf8_text(source_bytes) else {
                return;
            };
            if op_text != "??" && op_text != "||" {
                return;
            }
            let Some(left) = node.child_by_field_name("left") else {
                return;
            };
            if left.kind() != "identifier" {
                return;
            }
            let Ok(name) = left.utf8_text(source_bytes) else {
                return;
            };
            if !params.contains(name) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-nullish-default-on-input".into(),
                message: format!(
                    "Using '{op_text}' to default a function parameter '{name}' \
                     silently paves over invalid input. Validate at the \
                     boundary and return a Result error instead."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// Walk the tree and collect the names of every function parameter.
/// The result is a flat set — we don't track scopes, which means a local
/// variable sharing a param name would be a false positive. Accepted for
/// v1.1; scoped tracking is a future improvement.
fn collect_all_parameters(tree: &tree_sitter::Tree, source: &[u8]) -> HashSet<String> {
    let mut params = HashSet::new();
    walk_tree(tree, |node| {
        if node.kind() != "required_parameter" && node.kind() != "optional_parameter" {
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier"
                && let Ok(name) = child.utf8_text(source)
            {
                params.insert(name.to_string());
            }
        }
    });
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_nullish_coalesce_on_param() {
        assert_eq!(
            run_on("function f(x: number) { const v = x ?? 0; return v; }").len(),
            1
        );
    }

    #[test]
    fn flags_logical_or_on_param() {
        assert_eq!(
            run_on("function f(items: number[]) { const v = items || []; return v; }").len(),
            1
        );
    }

    #[test]
    fn allows_default_on_local_variable() {
        // `local` is not a parameter name in this file.
        assert!(run_on("function f() { const local: number | null = null; const v = local ?? 0; return v; }").is_empty());
    }

    #[test]
    fn allows_nullish_on_property_access() {
        assert!(
            run_on("function f(opts: { x?: number }) { return opts.x ?? 0; }").is_empty()
        );
    }
}
