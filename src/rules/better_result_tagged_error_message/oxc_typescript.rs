//! better-result-tagged-error-message oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["TaggedError"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Check if the class extends TaggedError.
        let Some(super_class) = &class.super_class else { return };
        let super_text =
            &ctx.source[super_class.span().start as usize..super_class.span().end as usize];
        if !super_text.contains("TaggedError") {
            return;
        }

        // Walk the class body looking for a `message` property.
        for element in &class.body.body {
            let oxc_ast::ast::ClassElement::PropertyDefinition(prop) = element else {
                continue;
            };
            let key_text =
                &ctx.source[prop.key.span().start as usize..prop.key.span().end as usize];
            if key_text == "message" {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Classes extending TaggedError must declare a `message: string` field.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
