//! node-handle-callback-err OXC backend — flag callback error params that are
//! never used.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, TSType, TSTypeName, TSTypeReference};
use std::sync::Arc;

pub struct Check;

fn is_error_param(name: &str) -> bool {
    name == "err" || name == "error" || name == "e"
}

/// Rightmost segment of a (possibly qualified) type name, e.g. `ErrnoException`
/// for `NodeJS.ErrnoException`.
fn type_reference_last_segment<'a>(r: &'a TSTypeReference<'a>) -> Option<&'a str> {
    match &r.type_name {
        TSTypeName::IdentifierReference(id) => Some(id.name.as_str()),
        TSTypeName::QualifiedName(q) => Some(q.right.name.as_str()),
        _ => None,
    }
}

/// `true` when a type reference names a genuine error type — `Error`, or any
/// name ending in `Error`/`Exception` (`AxiosError`, `NodeJS.ErrnoException`).
/// This keeps the inverse error-naming convention flaggable; it never widens
/// the candidate set beyond the name-matched parameter.
fn type_reference_is_error(r: &TSTypeReference) -> bool {
    type_reference_last_segment(r)
        .is_some_and(|name| name.ends_with("Error") || name.ends_with("Exception"))
}

/// A union member that signals the `Error | null` callback convention:
/// `null`, `undefined`, `any`/`unknown`, or an error-named reference.
fn union_member_signals_error(ty: &TSType) -> bool {
    match ty {
        TSType::TSNullKeyword(_)
        | TSType::TSUndefinedKeyword(_)
        | TSType::TSAnyKeyword(_)
        | TSType::TSUnknownKeyword(_) => true,
        TSType::TSTypeReference(r) => type_reference_is_error(r),
        TSType::TSUnionType(u) => u.types.iter().any(union_member_signals_error),
        _ => false,
    }
}

/// `true` when an explicit type annotation is still compatible with a Node
/// callback error parameter, so the unused-parameter diagnostic should stand:
/// `any`/`unknown`, an `Error`/`*Error`/`*Exception` reference, or a union
/// carrying any error-signalling member. Any other concrete type (`H3Event`,
/// `string`, an object type, …) describes a non-error value and suppresses the
/// diagnostic.
fn annotation_keeps_error_candidate(ty: &TSType) -> bool {
    match ty {
        TSType::TSAnyKeyword(_) | TSType::TSUnknownKeyword(_) => true,
        TSType::TSTypeReference(r) => type_reference_is_error(r),
        TSType::TSUnionType(u) => u.types.iter().any(union_member_signals_error),
        _ => false,
    }
}

/// Check if the function body source text references the given parameter name
/// as a standalone identifier.
fn body_uses_param(body_text: &str, param_name: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = body_text[start..].find(param_name) {
        let abs = start + pos;
        let before_ok = abs == 0 || {
            let prev = body_text.as_bytes()[abs - 1];
            !prev.is_ascii_alphanumeric() && prev != b'_'
        };
        let after_ok = {
            let after = abs + param_name.len();
            after >= body_text.len() || {
                let next = body_text.as_bytes()[after];
                !next.is_ascii_alphanumeric() && next != b'_'
            }
        };
        if before_ok && after_ok {
            return true;
        }
        start = abs + param_name.len();
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (params, body_span) = match node.kind() {
            AstKind::Function(func) => {
                let body = func.body.as_ref();
                let Some(body) = body else { return };
                (&func.params, body.span)
            }
            AstKind::ArrowFunctionExpression(arrow) => (&arrow.params, arrow.body.span),
            _ => return,
        };

        let Some(param) = params.items.first() else {
            return;
        };
        let BindingPattern::BindingIdentifier(id) = &param.pattern else {
            return;
        };
        let param_name = id.name.as_str();

        if !is_error_param(param_name) || param_name.starts_with('_') {
            return;
        }

        // A parameter explicitly typed as a non-error type (e.g. `e: H3Event`)
        // is not a Node callback error parameter, regardless of its name.
        if let Some(ann) = &param.type_annotation
            && !annotation_keeps_error_candidate(&ann.type_annotation)
        {
            return;
        }

        let body_text =
            &ctx.source[body_span.start as usize..body_span.end as usize];

        if !body_uses_param(body_text, param_name) {
            let span = match node.kind() {
                AstKind::Function(func) => func.span,
                AstKind::ArrowFunctionExpression(arrow) => arrow.span,
                _ => return,
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Callback error parameter `{param_name}` is declared but never used — handle the error."
                ),
                severity: Severity::Error,
                span: None,
            });
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Issue #6638 repro: non-error named type is not a callback error. ---

    #[test]
    fn skips_param_typed_as_non_error_named_type() {
        let src = "export function getPathRobotConfig(e: H3Event, options?: { path?: string }) { return { indexable: true }; }";
        assert_eq!(run_on(src).len(), 0, "{:?}", run_on(src));
    }

    #[test]
    fn skips_param_typed_as_primitive() {
        let src = "function f(e: string) { return 1; }";
        assert_eq!(run_on(src).len(), 0, "{:?}", run_on(src));
    }

    #[test]
    fn skips_param_typed_as_object_type() {
        let src = "function f(err: { path: string }) { return 1; }";
        assert_eq!(run_on(src).len(), 0, "{:?}", run_on(src));
    }

    #[test]
    fn skips_param_typed_as_non_error_union() {
        let src = "function f(e: string | number) { return 1; }";
        assert_eq!(run_on(src).len(), 0, "{:?}", run_on(src));
    }

    // --- Negative controls: error-compatible params STILL flag when unused. ---

    #[test]
    fn flags_untyped_err_param() {
        assert_eq!(run_on("function f(err, data) { return data; }").len(), 1);
    }

    #[test]
    fn flags_untyped_e_param_no_annotation() {
        assert_eq!(run_on("function f(e) { return 1; }").len(), 1);
    }

    #[test]
    fn flags_param_typed_any() {
        assert_eq!(run_on("function f(e: any) { return 1; }").len(), 1);
    }

    #[test]
    fn flags_param_typed_unknown() {
        assert_eq!(run_on("function f(e: unknown) { return 1; }").len(), 1);
    }

    #[test]
    fn flags_param_typed_error() {
        assert_eq!(run_on("function f(e: Error) { return 1; }").len(), 1);
    }

    #[test]
    fn flags_param_typed_qualified_errno_exception() {
        let src = "function f(err: NodeJS.ErrnoException) { return 1; }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn flags_param_typed_custom_error_suffix() {
        assert_eq!(run_on("function f(e: AxiosError) { return 1; }").len(), 1);
    }

    #[test]
    fn flags_param_typed_error_or_null_union() {
        assert_eq!(run_on("function f(e: Error | null) { return 1; }").len(), 1);
    }

    #[test]
    fn flags_param_typed_error_or_undefined_union() {
        let src = "function f(err: Error | undefined) { return 1; }";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // --- Used error param is never flagged regardless of annotation. ---

    #[test]
    fn does_not_flag_used_error_param() {
        assert_eq!(run_on("function f(err) { console.log(err); }").len(), 0);
    }

    #[test]
    fn does_not_flag_underscore_prefixed_param() {
        assert_eq!(run_on("function f(_err) { return 1; }").len(), 0);
    }

    #[test]
    fn does_not_flag_non_error_named_param() {
        assert_eq!(run_on("function f(data) { return 1; }").len(), 0);
    }
}
