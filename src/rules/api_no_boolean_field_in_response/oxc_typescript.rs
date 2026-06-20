//! api-no-boolean-field-in-response OXC backend — flag `boolean` properties
//! in response-shaped interfaces/type aliases. A name ending in a strong
//! response suffix (`Response`, `Dto`, `Reply`, `Body`) qualifies on its own;
//! the ambiguous suffixes `Result` and `Payload` qualify only when the shape
//! also carries a response-shaped field (`data`, `error`, `status`, …), so
//! library return types like `LoadCodegenConfigResult` and Redux action
//! payloads like `KeyboardTooltipActionPayload` are left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{PropertyKey, TSSignature, TSType};
use std::sync::Arc;

pub struct Check;

/// Suffixes that unambiguously mark an HTTP-response DTO. A name ending in one
/// of these is treated as a response type on its own.
const RESPONSE_SUFFIXES: &[&str] = &[
    "Response", "Dto", "DTO", "Reply", "Body",
];

/// Suffixes that match a response type in some contexts but mean something else
/// in others: `Result` is the generic convention for *any* function return type
/// (parsers, file readers, config loaders), and `Payload` is the Redux/event
/// action payload (`action.payload`) just as often as an HTTP response body. They
/// only count as a response type when the shape also carries a response-shaped
/// field.
const AMBIGUOUS_SUFFIXES: &[&str] = &["Result", "Payload"];

/// Field names typical of an HTTP-response envelope. Used to confirm an
/// ambiguously-suffixed type is actually a response shape.
const RESPONSE_SHAPED_FIELDS: &[&str] = &[
    "data", "error", "errors", "body", "headers", "status", "statusCode", "meta",
    "success", "message",
];

enum ResponseMatch {
    /// Strong suffix — fire on boolean fields unconditionally.
    Strong,
    /// Ambiguous suffix (`Result`/`Payload`) — fire only if a response-shaped
    /// field is present.
    Ambiguous,
    None,
}

fn classify_response_type(name: &str) -> ResponseMatch {
    if RESPONSE_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        ResponseMatch::Strong
    } else if AMBIGUOUS_SUFFIXES.iter().any(|s| name.ends_with(s)) {
        ResponseMatch::Ambiguous
    } else {
        ResponseMatch::None
    }
}

fn member_name<'a>(member: &'a TSSignature) -> Option<&'a str> {
    let TSSignature::TSPropertySignature(prop) = member else { return None };
    match &prop.key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

fn has_response_shaped_field(members: &[TSSignature]) -> bool {
    members.iter().filter_map(member_name).any(|name| {
        RESPONSE_SHAPED_FIELDS
            .iter()
            .any(|f| name.eq_ignore_ascii_case(f))
    })
}

/// Whether `members` should be checked for non-extensible boolean fields, given
/// how the declaration name matched. Ambiguous suffixes require a
/// response-shaped field; strong suffixes always qualify.
fn should_check(suffix_match: ResponseMatch, members: &[TSSignature]) -> bool {
    match suffix_match {
        ResponseMatch::Strong => true,
        ResponseMatch::Ambiguous => has_response_shaped_field(members),
        ResponseMatch::None => false,
    }
}

fn is_excluded_path(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("/scripts/")
        || s.starts_with("scripts/")
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
    

    #[test]
    fn no_fp_in_test_file() {
        let d = crate::rules::test_helpers::run_rule(&Check, "type LaboratoriesResponse = { items: string[]; replace: boolean };", "src/app/features/laboratories/components/laboratories-page.test.tsx");
        assert!(d.is_empty(), "unexpected diagnostics in test file: {d:?}");
    }

    #[test]
    fn no_fp_in_spec_file() {
        let d = crate::rules::test_helpers::run_rule(&Check, "type FooResponse = { created: boolean };", "src/foo.spec.ts");
        assert!(d.is_empty(), "unexpected diagnostics in spec file: {d:?}");
    }

    #[test]
    fn no_fp_in_scripts_dir() {
        let d = crate::rules::test_helpers::run_rule(&Check, "type SeedAdminCdrResponse = { user: string; created: boolean };", "scripts/seed-admin-cdr.ts");
        assert!(d.is_empty(), "unexpected diagnostics in scripts dir: {d:?}");
    }

    #[test]
    fn still_flags_in_regular_source_file() {
        let d = crate::rules::test_helpers::run_rule(&Check, "type SeedAdminCdrResponse = { user: string; created: boolean };", "src/api/seed-admin-cdr.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn no_fp_on_generic_result_without_response_field() {
        // Issue #3286: library return type, `Result` suffix, no response-shaped field.
        let d = crate::rules::test_helpers::run_rule(&Check, "interface LoadCodegenConfigResult { filepath: string; config: unknown; isEmpty?: boolean }", "src/config.ts");
        assert!(d.is_empty(), "generic Result type without a response-shaped field should not be flagged: {d:?}");
    }

    #[test]
    fn still_flags_strong_response_suffix() {
        let d = crate::rules::test_helpers::run_rule(&Check, "interface UserResponse { isActive: boolean }", "src/api/user.ts");
        assert_eq!(d.len(), 1, "strong response suffix must still fire standalone: {d:?}");
    }

    #[test]
    fn still_flags_result_with_response_shaped_field() {
        let d = crate::rules::test_helpers::run_rule(&Check, "interface FetchResult { data: unknown; error?: string; isSuccess: boolean }", "src/api/fetch.ts");
        assert_eq!(d.len(), 1, "a Result type carrying data/error is a real response: {d:?}");
    }

    #[test]
    fn no_fp_on_redux_action_payload() {
        // Issue #4754: Redux action payload, no response-shaped envelope field.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "export type KeyboardTooltipActionPayload = { active: boolean; activeIndex: number | undefined };",
            "src/state/tooltipSlice.ts",
        );
        assert!(d.is_empty(), "Redux action payload without a response-shaped field should not be flagged: {d:?}");
    }

    #[test]
    fn no_fp_on_chart_legend_payload() {
        // Issue #4754: chart component data shape, `inactive` is a visibility toggle.
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "export interface LegendPayload { value: string | undefined; inactive?: boolean }",
            "src/component/DefaultLegendContent.tsx",
        );
        assert!(d.is_empty(), "chart legend payload without a response-shaped field should not be flagged: {d:?}");
    }

    #[test]
    fn still_flags_payload_with_response_shaped_field() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "interface UserPayload { data: unknown; status: number; isActive: boolean }",
            "src/api/user.ts",
        );
        assert_eq!(d.len(), 1, "a Payload type carrying data/status is a real response: {d:?}");
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
        if is_excluded_path(ctx.path) {
            return;
        }
        match node.kind() {
            AstKind::TSInterfaceDeclaration(decl) => {
                let m = classify_response_type(decl.id.name.as_str());
                if should_check(m, &decl.body.body) {
                    check_members(&decl.body.body, ctx, diagnostics);
                }
            }
            AstKind::TSTypeAliasDeclaration(decl) => {
                let m = classify_response_type(decl.id.name.as_str());
                if let TSType::TSTypeLiteral(obj) = &decl.type_annotation {
                    if should_check(m, &obj.members) {
                        check_members(&obj.members, ctx, diagnostics);
                    }
                }
            }
            _ => {}
        }
    }
}
