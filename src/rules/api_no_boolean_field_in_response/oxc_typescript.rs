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
        let d = crate::rules::test_helpers::run_rule(&Check, "type FooResult = { created: boolean };", "src/foo.spec.ts");
        assert!(d.is_empty(), "unexpected diagnostics in spec file: {d:?}");
    }

    #[test]
    fn no_fp_in_scripts_dir() {
        let d = crate::rules::test_helpers::run_rule(&Check, "type SeedAdminCdrResult = { user: string; created: boolean };", "scripts/seed-admin-cdr.ts");
        assert!(d.is_empty(), "unexpected diagnostics in scripts dir: {d:?}");
    }

    #[test]
    fn still_flags_in_regular_source_file() {
        let d = crate::rules::test_helpers::run_rule(&Check, "type SeedAdminCdrResult = { user: string; created: boolean };", "src/api/seed-admin-cdr.ts");
        assert_eq!(d.len(), 1);
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
