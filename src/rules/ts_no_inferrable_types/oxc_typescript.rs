//! ts-no-inferrable-types OXC backend — flag variable declarations where the
//! type annotation is trivially inferred from a literal initializer.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Map from TS type keyword to the literal expression kind it's inferred from.
fn inferrable_type_name(annotation: &str, init: &Expression) -> Option<&'static str> {
    match (annotation, init) {
        ("number", Expression::NumericLiteral(_)) => Some("number"),
        ("string", Expression::StringLiteral(_)) => Some("string"),
        ("string", Expression::TemplateLiteral(_)) => Some("string"),
        ("boolean", Expression::BooleanLiteral(_)) => Some("boolean"),
        ("null", Expression::NullLiteral(_)) => Some("null"),
        ("undefined", Expression::Identifier(id)) if id.name.as_str() == "undefined" => {
            Some("undefined")
        }
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        for declarator in &decl.declarations {
            let Some(ref init) = declarator.init else {
                continue;
            };
            let Some(ref type_ann) = declarator.type_annotation else {
                continue;
            };

            // Extract the type annotation text (skip the leading `: `)
            let ann_text = &ctx.source
                [type_ann.type_annotation.span().start as usize..type_ann.type_annotation.span().end as usize];

            if let Some(type_name) = inferrable_type_name(ann_text, init) {
                let (line, column) = byte_offset_to_line_col(
                    ctx.source,
                    type_ann.span.start as usize,
                );
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Type `{type_name}` is trivially inferred from the literal — \
                         remove the type annotation."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}
