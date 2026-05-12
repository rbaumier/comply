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
