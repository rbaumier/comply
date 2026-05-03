//! OXC backend for ts-no-mixed-decorator-systems.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["reflect-metadata"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut has_reflect_metadata = false;
        let mut first_decorator_span: Option<oxc_span::Span> = None;

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let src = import.source.value.as_str();
                    if src == "reflect-metadata" {
                        has_reflect_metadata = true;
                    }
                }
                AstKind::Class(class) => {
                    if first_decorator_span.is_none() && !class.decorators.is_empty() {
                        first_decorator_span = Some(class.decorators[0].span);
                    }
                }
                AstKind::MethodDefinition(method) => {
                    if first_decorator_span.is_none() && !method.decorators.is_empty() {
                        first_decorator_span = Some(method.decorators[0].span);
                    }
                }
                AstKind::PropertyDefinition(prop) => {
                    if first_decorator_span.is_none() && !prop.decorators.is_empty() {
                        first_decorator_span = Some(prop.decorators[0].span);
                    }
                }
                _ => {}
            }
        }

        if !has_reflect_metadata {
            return Vec::new();
        }
        let Some(span) = first_decorator_span else {
            return Vec::new();
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "File mixes decorators with a `reflect-metadata` import — standard and experimental decorator systems cannot coexist.".into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}
