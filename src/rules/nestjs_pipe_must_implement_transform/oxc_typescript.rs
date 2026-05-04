//! nestjs-pipe-must-implement-transform — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, PropertyKey, TSTypeName};
use std::sync::Arc;

pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    source.contains("@nestjs/") || source.contains("PipeTransform")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["PipeTransform"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nestjs_file(ctx.source) {
            return;
        }
        let AstKind::Class(class) = node.kind() else { return };

        // Check if class implements PipeTransform.
        let implements_pipe = class.implements.iter().any(|impl_clause| {
            matches!(&impl_clause.expression, TSTypeName::IdentifierReference(id) if id.name.as_str() == "PipeTransform")
        });
        if !implements_pipe {
            return;
        }

        // Check if class has a `transform` method.
        let has_transform = class.body.body.iter().any(|element| {
            if let ClassElement::MethodDefinition(method) = element {
                matches!(&method.key, PropertyKey::StaticIdentifier(id) if id.name.as_str() == "transform")
            } else {
                false
            }
        });
        if has_transform {
            return;
        }

        let Some(id) = &class.id else { return };
        let name = id.name.as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Class `{name}` implements `PipeTransform` but is missing the required `transform()` method."),
            severity: Severity::Warning,
            span: None,
        });
    }
}
