//! no-magic-numbers OxcCheck backend — flag numeric literals that are not in
//! an allowed context (const declarations, enums, type annotations,
//! default parameter values, array indices 0/1/-1).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Numeric values so idiomatic that flagging them is pure noise.
const ALLOWED: &[&str] = &["-1", "0", "1", "2", "0.0", "1.0"];

/// HTTP status codes — universally understood, extracting them to a constant
/// makes the code less readable, not more.
const HTTP_STATUS_CODES: &[f64] = &[
    200.0, 201.0, 204.0, 301.0, 302.0, 304.0, 400.0, 401.0, 403.0, 404.0,
    405.0, 409.0, 422.0, 429.0, 500.0, 502.0, 503.0,
];

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NumericLiteral(num) = node.kind() else {
            return;
        };

        if ctx.file.path_segments.in_test_dir {
            return;
        }
        if ctx.path.to_string_lossy().contains("/examples/") {
            return;
        }

        let text = &ctx.source[num.span.start as usize..num.span.end as usize];

        // Allow universally understood values.
        if ALLOWED.contains(&text) {
            return;
        }
        if HTTP_STATUS_CODES.contains(&num.value) {
            return;
        }

        // Check for unary minus: parent is UnaryExpression with "-".
        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id != node.id()
            && let AstKind::UnaryExpression(unary) = nodes.get_node(parent_id).kind()
                && unary.operator == oxc_ast::ast::UnaryOperator::UnaryNegation {
                    let parent_text =
                        &ctx.source[unary.span.start as usize..unary.span.end as usize];
                    if ALLOWED.contains(&parent_text) {
                        return;
                    }
                }

        if is_allowed_context(node.id(), semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, num.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Magic number `{text}` — extract into a named constant."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_allowed_context(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    let mut current_id = node_id;

    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return false;
        }
        let parent = nodes.get_node(parent_id);

        match parent.kind() {
            // const declaration initializer
            AstKind::VariableDeclarator(_) => {
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id
                    && let AstKind::VariableDeclaration(decl) = nodes.get_node(gp_id).kind()
                        && decl.kind == oxc_ast::ast::VariableDeclarationKind::Const {
                            return true;
                        }
            }
            // Enum member value
            AstKind::TSEnumMember(_) | AstKind::TSEnumBody(_) | AstKind::TSEnumDeclaration(_) => {
                return true;
            }
            // Type annotation / type literal
            AstKind::TSTypeAnnotation(_) | AstKind::TSLiteralType(_) => return true,
            // Default parameter value
            AstKind::FormalParameter(_) => return true,
            // Class property (readonly or not — the TS version allows all)
            AstKind::PropertyDefinition(_) => return true,
            // Array index access (subscript expression)
            AstKind::ComputedMemberExpression(computed) => {
                // Check if this number is the index expression
                let num_node = nodes.get_node(current_id);
                let num_span = match num_node.kind() {
                    AstKind::NumericLiteral(n) => n.span,
                    AstKind::UnaryExpression(u) => u.span,
                    _ => return false,
                };
                if computed.expression.span() == num_span {
                    return true;
                }
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_magic_number() {
        let diags = run_on("const timeout = getTimeout(); if (timeout > 3000) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3000"));
    }


    #[test]
    fn allows_const_declaration() {
        assert!(run_on("const MAX_TIMEOUT = 3000;").is_empty());
    }


    #[test]
    fn allows_zero_and_one() {
        assert!(run_on("const arr = items[0]; const len = arr.length - 1;").is_empty());
    }


    #[test]
    fn allows_enum_values() {
        assert!(run_on("enum Status { Active = 200, Error = 500 }").is_empty());
    }

    #[test]
    fn allowlist_zero() {
        assert!(run_on("function f(x) { return x + 0; }").is_empty());
    }


    #[test]
    fn allowlist_one() {
        assert!(run_on("function f(x) { return x + 1; }").is_empty());
    }


    #[test]
    fn allowlist_minus_one_unary() {
        // `-1` parses as unary_expression(- , number(1)) — the walker
        // must recognise the wrapped form, not just a literal token.
        assert!(run_on("function f(x) { return x + -1; }").is_empty());
    }

    #[test]
    fn flags_bare_forty_two() {
        let diags = run_on("function f(x) { return x + 42; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("42"));
    }


    #[test]
    fn flags_negative_forty_two() {
        let diags = run_on("function f(x) { return x + -42; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("42"));
    }


    #[test]
    fn allows_two() {
        assert!(run_on("function f(x) { return x + 2; }").is_empty());
    }


    #[test]
    fn allows_http_status_codes() {
        assert!(run_on("if (res.status === 200) {}").is_empty());
        assert!(run_on("if (res.status === 404) {}").is_empty());
        assert!(run_on("if (res.status === 500) {}").is_empty());
    }


    #[test]
    fn still_flags_non_standard_numbers() {
        assert_eq!(run_on("function f(x) { return x + 42; }").len(), 1);
    }


    #[test]
    fn ignores_numbers_inside_string_literals() {
        // Tailwind utilities like `"p-4"` are CSS class names, not
        // programmatic magic numbers. The AST only exposes a `string`
        // node here — no `number` child is walked, so nothing to skip.
        assert!(run_on(r#"function f() { return "p-4 mb-6 h-2.5"; }"#).is_empty());
    }


    #[test]
    fn flags_both_operands_of_arithmetic() {
        let diags = run_on("function f(x) { return x + 5 * 60; }");
        assert_eq!(diags.len(), 2);
    }
}
