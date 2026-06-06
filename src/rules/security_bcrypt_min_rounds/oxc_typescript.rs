//! security-bcrypt-min-rounds OXC backend — flag bcrypt hashing with cost < 12.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Resolve a `const FOO = <number>` binding from the semantic scope.
fn resolve_const_number(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> Option<i64> {
    for node in semantic.nodes().iter() {
        if let AstKind::VariableDeclarator(decl) = node.kind() {
            if let oxc_ast::ast::BindingPattern::BindingIdentifier(binding) = &decl.id {
                if binding.name.as_str() != ident.name.as_str() {
                    continue;
                }
            } else {
                continue;
            }
            // Check it's a const declaration.
            let parent = semantic.nodes().parent_node(node.id());
            if let AstKind::VariableDeclaration(var_decl) = parent.kind() {
                if var_decl.kind != oxc_ast::ast::VariableDeclarationKind::Const {
                    continue;
                }
            } else {
                continue;
            }
            if let Some(Expression::NumericLiteral(num)) = &decl.init {
                return Some(num.value as i64);
            }
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["bcrypt"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Must be `bcrypt.hash` / `bcrypt.hashSync` / `bcryptjs.hash` / `bcryptjs.hashSync`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if method != "hash" && method != "hashSync" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        let obj_name = obj.name.as_str();
        if obj_name != "bcrypt" && obj_name != "bcryptjs" {
            return;
        }

        let fn_name = format!("{obj_name}.{method}");

        // Second argument is the cost factor.
        let Some(cost_arg) = call.arguments.get(1) else {
            return;
        };
        let Some(cost_expr) = cost_arg.as_expression() else {
            return;
        };

        let value: i64 = match cost_expr {
            Expression::NumericLiteral(num) => num.value as i64,
            Expression::Identifier(ident) => {
                let Some(v) = resolve_const_number(ident, semantic) else {
                    return;
                };
                v
            }
            _ => return,
        };

        if value >= 12 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{fn_name}` cost factor {value} is below 12 — use at least 12 to resist brute-force attacks."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}
