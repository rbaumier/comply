//! better-result-no-param-properties oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, MethodDefinitionKind};
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
        let AstKind::Class(class) = node.kind() else {
            return;
        };

        // Check if extends TaggedError.
        let Some(super_class) = &class.super_class else {
            return;
        };
        let super_text =
            &ctx.source[super_class.span().start as usize..super_class.span().end as usize];
        if !super_text.contains("TaggedError") {
            return;
        }

        // Find constructor and check for parameter properties.
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Constructor {
                continue;
            }
            for param in &method.value.params.items {
                if param.accessibility.is_some() || param.readonly {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, param.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Parameter property not allowed on TaggedError constructor — assign via super({ ...args, message }).".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}
