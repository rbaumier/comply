use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, TSType};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(declarator) = node.kind() else {
            return;
        };
        // Must have a `new Foo()` init.
        let Some(init) = &declarator.init else { return };
        let Expression::NewExpression(new_expr) = init else {
            return;
        };
        // The new expression must NOT have type arguments.
        if new_expr.type_arguments.is_some() {
            return;
        }
        // Must have a type annotation with type arguments on a TSTypeReference.
        let Some(type_ann) = &declarator.type_annotation else {
            return;
        };
        let TSType::TSTypeReference(ref_ty) = &type_ann.type_annotation else {
            return;
        };
        if ref_ty.type_arguments.is_none() {
            return;
        }
        // Verify constructor name matches type name.
        let constructor_name = match &new_expr.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            _ => None,
        };
        let type_name = match &ref_ty.type_name {
            oxc_ast::ast::TSTypeName::IdentifierReference(ident) => {
                Some(ident.name.as_str())
            }
            _ => None,
        };
        if let (Some(cn), Some(tn)) = (constructor_name, type_name)
            && cn != tn {
                return;
            }
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            return;
        };
        let (line, column) =
            byte_offset_to_line_col(ctx.source, id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Generic type arguments should be specified on the constructor, not the type annotation.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
