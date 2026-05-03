//! api-no-boolean-field-in-response OXC backend — flag `boolean` properties
//! in response-shaped interfaces/type aliases.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSSignature, TSType};
use std::sync::Arc;

pub struct Check;

const RESPONSE_SUFFIXES: &[&str] = &[
    "Response", "Dto", "DTO", "Payload", "Reply", "Result", "Body",
];

fn looks_like_response_type(name: &str) -> bool {
    RESPONSE_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn is_plain_boolean(ts_type: &TSType) -> bool {
    matches!(ts_type, TSType::TSBooleanKeyword(_))
}

fn check_members(
    members: &[TSSignature],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for member in members {
        let TSSignature::TSPropertySignature(prop) = member else { continue };
        let Some(ref type_ann) = prop.type_annotation else { continue };
        if !is_plain_boolean(&type_ann.type_annotation) {
            continue;
        }
        let prop_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            _ => "<field>",
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Response field `{prop_name}: boolean` is not extensible \u{2014} prefer a string-union / enum so new states don't break clients."
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
                if !looks_like_response_type(decl.id.name.as_str()) {
                    return;
                }
                check_members(&decl.body.body, ctx, diagnostics);
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                if !looks_like_response_type(decl.id.name.as_str()) {
                    return;
                }
                if let TSType::TSTypeLiteral(obj) = &decl.type_annotation {
                    check_members(&obj.members, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}
