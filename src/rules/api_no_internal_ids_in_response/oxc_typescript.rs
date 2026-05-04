//! api-no-internal-ids-in-response OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSSignature;
use std::sync::Arc;

pub struct Check;

const RESPONSE_SUFFIXES: &[&str] = &[
    "Response", "Dto", "DTO", "Payload", "Reply", "Result", "Body", "Output", "View",
];

fn is_response_type(name: &str) -> bool {
    RESPONSE_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn is_internal_field(name: &str) -> bool {
    if name == "pk" || name == "rowid" || name == "oid" {
        return true;
    }
    if name.starts_with("internal_") || name.starts_with("internal") && name.len() > 8 {
        let rest = &name[8..];
        if rest.starts_with('_') || rest.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return true;
        }
    }
    if name.ends_with("_id") && name.len() > 3 {
        return true;
    }
    false
}

fn check_members(
    members: &oxc_ast::ast::TSInterfaceBody,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for member in &members.body {
        if let TSSignature::TSPropertySignature(prop) = member {
            let name = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                _ => continue,
            };
            if !is_internal_field(name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Response field `{name}` looks internal — rename to its public form or drop it from the DTO."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

fn check_object_type(
    obj: &oxc_ast::ast::TSTypeLiteral,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for member in &obj.members {
        if let TSSignature::TSPropertySignature(prop) = member {
            let name = match &prop.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                _ => continue,
            };
            if !is_internal_field(name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Response field `{name}` looks internal — rename to its public form or drop it from the DTO."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
                if !is_response_type(decl.id.name.as_str()) {
                    return;
                }
                check_members(&decl.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if !is_response_type(decl.id.name.as_str()) {
                    return;
                }
                if let oxc_ast::ast::TSType::TSTypeLiteral(obj) = &decl.type_annotation {
                    check_object_type(obj, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}
