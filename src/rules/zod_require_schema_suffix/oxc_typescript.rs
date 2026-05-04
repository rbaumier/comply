//! zod-require-schema-suffix OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Unwrap call chains (`z.object({}).strict()`) to find the root expression.
fn chain_root<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    match expr {
        Expression::CallExpression(call) => chain_root(&call.callee),
        Expression::StaticMemberExpression(member) => {
            // Stop when the object is an identifier — this member_expression
            // is the root (e.g. `z.object`).
            if matches!(&member.object, Expression::Identifier(_)) {
                return expr;
            }
            chain_root(&member.object)
        }
        _ => expr,
    }
}

/// Whether `expr` is a call chain starting with `z.<anything>`.
fn starts_with_z(expr: &Expression) -> bool {
    let root = chain_root(expr);
    if let Expression::StaticMemberExpression(member) = root {
        if let Expression::Identifier(id) = &member.object {
            return id.name.as_str() == "z";
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        // Must be exported — check parent node.
        let parent = semantic.nodes().parent_node(node.id());
        let is_exported = matches!(
            parent.kind(),
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
        );
        if !is_exported {
            return;
        }

        for declarator in &decl.declarations {
            let name = match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id.name.as_str(),
                _ => continue,
            };
            if name.ends_with("Schema") {
                continue;
            }
            let Some(init) = &declarator.init else {
                continue;
            };
            if !starts_with_z(init) {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, declarator.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Exported Zod schema `{name}` should be renamed `{name}Schema` — \
                     the suffix keeps the schema distinguishable from the inferred \
                     TypeScript type."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}
