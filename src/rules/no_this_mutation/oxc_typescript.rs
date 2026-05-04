use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, MethodDefinitionKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AssignmentExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AssignmentExpression(assign) = node.kind() else { return };

        // Check if the left side is `this.something`
        let oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) = &assign.left else {
            return;
        };
        if !matches!(&member.object, Expression::ThisExpression(_)) {
            return;
        }

        // Walk ancestors to determine if we're inside a constructor
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::MethodDefinition(method) => {
                    if method.kind == MethodDefinitionKind::Constructor {
                        return; // Inside constructor, allowed
                    }
                    break; // Inside a method but not constructor
                }
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    break; // Inside a regular function, not a method
                }
                AstKind::PropertyDefinition(_) => {
                    // Direct assignment in class body (field initializer) is OK
                    return;
                }
                _ => {}
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, assign.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Mutation of `this` outside constructor \u{2014} initialize properties in constructor.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
