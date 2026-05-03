//! OxcCheck backend for ts-no-misused-new.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, TSSignature};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class, AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Class(class) => {
                // Flag `new()` method in class body
                for element in &class.body.body {
                    let ClassElement::MethodDefinition(method) = element else { continue };
                    let name = method.key.static_name();
                    if name.as_deref() != Some("new") {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, method.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Class cannot have method named `new` — use `constructor` instead."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                // Flag `constructor()` method in interface body
                for sig in &iface.body.body {
                    let TSSignature::TSMethodSignature(method) = sig else { continue };
                    let name = method.key.static_name();
                    if name.as_deref() != Some("constructor") {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, method.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Interfaces cannot be constructed — use `new(): Type` instead of `constructor()`."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
