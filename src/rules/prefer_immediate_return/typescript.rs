//! prefer-immediate-return TS / JS / TSX backend.
//!
//! Flag `const x = expr; return x;` inside a block — simplifiable to
//! `return expr;`. Detection is AST-based: walks `statement_block`
//! nodes and looks at consecutive children, never at source lines.
//! JS/TS don't have implicit-return tail expressions, so only the
//! explicit `return X;` form is matched.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["statement_block"])
    }

    fn visit_node(
        &self,
        block: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let mut cursor = block.walk();
        let children: Vec<_> = block.named_children(&mut cursor).collect();
        for i in 0..children.len().saturating_sub(1) {
            let decl_node = children[i];
            let next_node = children[i + 1];
            if !matches!(
                decl_node.kind(),
                "lexical_declaration" | "variable_declaration"
            ) {
                continue;
            }
            let Some(var_name) = extract_single_declarator_name(decl_node, source_bytes) else {
                continue;
            };
            if next_node.kind() != "return_statement" {
                continue;
            }
            if !return_value_is_identifier(next_node, source_bytes, var_name) {
                continue;
            }
            let pos = decl_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-immediate-return".into(),
                message: format!(
                    "Variable `{var_name}` is assigned and immediately \
                     returned — return the expression directly."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Return the single identifier name bound by a `const X = …` /
/// `let X = …` / `var X = …` declaration. Returns `None` for
/// destructuring (`const { x } = …`, `const [x] = …`) or when the
/// declaration binds more than one name.
fn extract_single_declarator_name<'a>(
    node: tree_sitter::Node,
    source: &'a [u8],
) -> Option<&'a str> {
    let mut cursor = node.walk();
    let decls: Vec<_> = node
        .named_children(&mut cursor)
        .filter(|n| n.kind() == "variable_declarator")
        .collect();
    if decls.len() != 1 {
        return None;
    }
    let name = decls[0].child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    name.utf8_text(source).ok()
}

fn return_value_is_identifier(return_node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut cursor = return_node.walk();
    let Some(value) = return_node.named_children(&mut cursor).next() else {
        return false;
    };
    value.kind() == "identifier" && value.utf8_text(source).ok() == Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_const_then_return() {
        let src = "function f() { const result = computeValue(); return result; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_let_then_return() {
        let src = "function f() { let x = a + b; return x; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_assign_used_later() {
        let src =
            "function f() { const result = computeValue(); console.log(result); return result; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_different_variable_returned() {
        let src = "function f() { const result = computeValue(); return other; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_destructuring() {
        let src = "function f() { const { a, b } = getValues(); return a; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_method_chain_on_next_statement() {
        // TS equivalent of the user's FP: declaration followed by a
        // method chain on the same variable, then a return.
        let src = r#"
            function run() {
                const parser = new Parser();
                parser.setLanguage(Lang.TypeScript);
                return parser;
            }
        "#;
        assert!(run(src).is_empty());
    }
}
