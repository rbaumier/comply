//! ts-method-signature-style oxc backend — flag shorthand method signatures
//! in interfaces and type literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSSignature};
use std::sync::Arc;

pub struct Check;

fn key_name<'a>(key: &'a PropertyKey<'a>) -> &'a str {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str(),
        PropertyKey::StringLiteral(s) => s.value.as_str(),
        _ => "method",
    }
}

fn report_method_signatures<'a>(
    members: &[TSSignature<'a>],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for sig in members {
        let TSSignature::TSMethodSignature(method) = sig else { continue };
        let name = key_name(&method.key);
        let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Shorthand method signature `{name}(...)` is less safe — \
                 use a property signature: `{name}: (...) => ReturnType`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSInterfaceDeclaration, AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) => {
                report_method_signatures(&decl.body.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if let oxc_ast::ast::TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                    report_method_signatures(&lit.members, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}
