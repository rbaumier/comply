//! nestjs-no-missing-injectable oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nestjs_file(source: &str) -> bool {
    source.contains("@nestjs/")
}

fn class_name_looks_like_provider(name: &str) -> bool {
    name.ends_with("Service")
        || name.ends_with("Repository")
        || name.ends_with("UseCase")
        || name.ends_with("Provider")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@nestjs/"])
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
        if !is_nestjs_file(ctx.source) {
            return;
        }

        let Some(id) = &class.id else { return };
        let name = id.name.as_str();
        if !class_name_looks_like_provider(name) {
            return;
        }

        // Check if class has @Injectable() decorator.
        if ctx.source.contains("@Injectable") {
            // Walk decorators on the class.
            for decorator in &class.decorators {
                let dec_start = decorator.span.start as usize;
                let dec_end = decorator.span.end as usize;
                let dec_text = &ctx.source[dec_start..dec_end];
                if dec_text.contains("@Injectable") {
                    return;
                }
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Class `{name}` looks like a NestJS provider but is missing `@Injectable()`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
