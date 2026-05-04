//! OXC backend for ts-explicit-member-accessibility — flag class members
//! (methods and properties) that lack an explicit accessibility modifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, MethodDefinitionKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        for elem in &class.body.body {
            match elem {
                ClassElement::MethodDefinition(method) => {
                    // Skip constructors — they don't need accessibility.
                    if method.kind == MethodDefinitionKind::Constructor {
                        continue;
                    }
                    // `#name` private identifiers are already explicitly private.
                    if matches!(&method.key, PropertyKey::PrivateIdentifier(_)) {
                        continue;
                    }
                    if method.accessibility.is_some() {
                        continue;
                    }
                    let name = property_key_name(&method.key, ctx.source);
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, method.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Class member '{}' is missing an accessibility modifier. \
                             Add `public`, `private`, or `protected`.",
                            name
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                ClassElement::PropertyDefinition(prop) => {
                    if matches!(&prop.key, PropertyKey::PrivateIdentifier(_)) {
                        continue;
                    }
                    if prop.accessibility.is_some() {
                        continue;
                    }
                    let name = property_key_name(&prop.key, ctx.source);
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, prop.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Class member '{}' is missing an accessibility modifier. \
                             Add `public`, `private`, or `protected`.",
                            name
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                _ => {}
            }
        }
    }
}

fn property_key_name<'a>(key: &'a PropertyKey<'a>, source: &'a str) -> &'a str {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
        _ => {
            let span = key.span();
            let start = span.start as usize;
            let end = span.end as usize;
            if end <= source.len() {
                &source[start..end]
            } else {
                "<member>"
            }
        }
    }
}
