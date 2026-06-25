//! OxcCheck backend for ts-no-large-string-union — flag unions with >N string-literal members.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{TSLiteral, TSType};
use std::sync::Arc;

pub struct Check;

/// Count only string-literal union members. Numeric/boolean literals (e.g. HTTP
/// status-code unions) are not what a branded string or enum would replace, so
/// they must not count toward the threshold.
fn count_string_literals(ty: &TSType) -> usize {
    match ty {
        TSType::TSUnionType(union) => union.types.iter().map(count_string_literals).sum(),
        TSType::TSLiteralType(lit) => matches!(lit.literal, TSLiteral::StringLiteral(_)) as usize,
        _ => 0,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeAliasDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else { return };

        let TSType::TSUnionType(union) = &alias.type_annotation else { return };

        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        let count: usize = union.types.iter().map(count_string_literals).sum();

        if count > max {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, union.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "String-literal union has {count} members (>{max}); consider a branded string or enum."
                ),
                severity: Severity::Warning,
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

    fn string_union(n: usize) -> String {
        let members: Vec<String> = (0..n).map(|i| format!("'s{i}'")).collect();
        format!("type T = {};", members.join(" | "))
    }

    fn numeric_union(n: usize) -> String {
        let members: Vec<String> = (0..n).map(|i| i.to_string()).collect();
        format!("type T = {};", members.join(" | "))
    }

    #[test]
    fn exempts_numeric_dominated_http_status_union() {
        // openapi-typescript ErrorStatus: ~45 numeric codes, 3 string members.
        let src = r#"
export type ErrorStatus =
  500 | 501 | 502 | 503 | 504 | 505 | 506 | 507 | 508 | 510 | 511 | '5XX' |
  400 | 401 | 402 | 403 | 404 | 405 | 406 | 407 | 408 | 409 | 410 | 411 | 412 |
  413 | 414 | 415 | 416 | 417 | 418 | 420 | 421 | 422 | 423 | 424 | 425 | 426 |
  427 | 428 | 429 | 430 | 431 | 444 | 450 | 451 | 497 | 498 | 499 | '4XX' | "default";
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn exempts_large_purely_numeric_union() {
        assert!(run_on(&numeric_union(60)).is_empty());
    }

    #[test]
    fn flags_large_string_union() {
        assert_eq!(run_on(&string_union(60)).len(), 1);
    }

    #[test]
    fn flags_mixed_union_with_enough_string_members() {
        // 51 string members + 10 numeric members: still over the 50 threshold.
        let strings: Vec<String> = (0..51).map(|i| format!("'s{i}'")).collect();
        let numbers: Vec<String> = (0..10).map(|i| i.to_string()).collect();
        let src = format!("type T = {} | {};", strings.join(" | "), numbers.join(" | "));
        assert_eq!(run_on(&src).len(), 1);
    }

    #[test]
    fn ignores_small_string_union() {
        assert!(run_on(&string_union(50)).is_empty());
    }
}
