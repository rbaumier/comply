//! OxcCheck backend for no-valueof-field.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["valueOf"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                // Class method: class Foo { valueOf() {} }
                AstKind::MethodDefinition(method) => {
                    if let PropertyKey::StaticIdentifier(id) = &method.key {
                        if id.name == "valueOf" {
                            push(&mut diagnostics, ctx, id.span);
                        }
                    }
                }
                // Interface method signature: interface Foo { valueOf(): number }
                AstKind::TSMethodSignature(sig) => {
                    if let PropertyKey::StaticIdentifier(id) = &sig.key {
                        if id.name == "valueOf" {
                            push(&mut diagnostics, ctx, id.span);
                        }
                    }
                }
                // Interface/type property signature: interface Foo { valueOf: () => number }
                AstKind::TSPropertySignature(sig) => {
                    if let PropertyKey::StaticIdentifier(id) = &sig.key {
                        if id.name == "valueOf" {
                            push(&mut diagnostics, ctx, id.span);
                        }
                    }
                }
                // Object property: { valueOf: function() {} } or { valueOf: () => {} }
                AstKind::ObjectProperty(prop) => {
                    if let PropertyKey::StaticIdentifier(id) = &prop.key {
                        if id.name == "valueOf" {
                            // Only flag if value is a function.
                            if prop.method
                                || matches!(
                                    prop.value,
                                    Expression::ArrowFunctionExpression(_)
                                        | Expression::FunctionExpression(_)
                                )
                            {
                                push(&mut diagnostics, ctx, id.span);
                            }
                        }
                    }
                }
                // Class field: class Foo { valueOf = () => 1 }
                AstKind::PropertyDefinition(field) => {
                    if let PropertyKey::StaticIdentifier(id) = &field.key {
                        if id.name == "valueOf" {
                            push(&mut diagnostics, ctx, id.span);
                        }
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn push(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span: oxc_span::Span) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Do not override `valueOf`. Use an explicit conversion method instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}
