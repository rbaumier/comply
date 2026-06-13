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

/// Field names dictated by external wire protocols (OAuth 2.0 / OpenID Connect
/// error responses, Firebase Cloud Messaging payloads). They are snake_case
/// because the protocol mandates it, not because they leak a DB column, and
/// they cannot be renamed without breaking interop.
const STANDARD_PROTOCOL_FIELDS: &[&str] = &[
    // OAuth 2.0 / OpenID Connect error-response correlation fields.
    "trace_id",
    "correlation_id",
    "request_id",
    "session_id",
    "session_state",
    // Firebase Cloud Messaging push-notification payload fields.
    "android_channel_id",
    "google_message_id",
    "message_id",
];

fn is_standard_protocol_field(name: &str) -> bool {
    STANDARD_PROTOCOL_FIELDS.contains(&name)
}

fn is_internal_field(name: &str) -> bool {
    if is_standard_protocol_field(name) {
        return false;
    }
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

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_snake_case_foreign_key() {
        let d = run("interface OrderResponse { user_id: string; total: number }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("user_id"));
    }

    #[test]
    fn flags_genuinely_internal_id_fields() {
        let d = run("interface UserResponse { user_db_id: string; internal_id: number }");
        assert_eq!(d.len(), 2, "{d:?}");
    }

    #[test]
    fn allows_oauth_error_correlation_fields_issue_1152() {
        // Azure AAD OAuth 2.0 error response — wire-protocol fields, not DB IDs.
        let d = run(
            "interface OAuthErrorResponse { error: string; trace_id?: string; correlation_id?: string }",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn allows_fcm_android_channel_id_issue_1152() {
        // Firebase Cloud Messaging push payload — spec-mandated field name.
        let d = run("interface NotificationPayload { android_channel_id?: string }");
        assert!(d.is_empty(), "{d:?}");
    }
}
