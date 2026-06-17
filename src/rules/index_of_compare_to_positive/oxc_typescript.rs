//! index-of-compare-to-positive — oxc backend.
//!
//! Flags `.indexOf(x) < 1`, which is ambiguously `=== 0 || === -1` and almost
//! always a forgotten-`-1` bug. The intended check is `< 0` (absent) or `!== -1`
//! (present).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BinaryOperator, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["indexOf"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        let op = bin.operator;
        if op != BinaryOperator::LessThan {
            return;
        }

        let right_text =
            &ctx.source[bin.right.span().start as usize..bin.right.span().end as usize];
        let right_text = right_text.trim();

        // `.indexOf(…) < 1` is ambiguously `=== 0 || === -1` — a likely forgotten
        // `-1` bug. `> 0` is intentionally excluded: it is a valid "present at a
        // non-leading position" check, not a bug.
        if right_text != "1" {
            return;
        }

        // Check if left side is a `.indexOf(...)` call.
        let Expression::CallExpression(call) = &bin.left else { return };
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "indexOf" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.indexOf(…) < 1` matches both index 0 and absence — use `< 0` or `!== -1`."
                .into(),
            severity: Severity::Error,
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_indexof_lt_one() {
        assert_eq!(run_on("if (str.indexOf('a') < 1) {}").len(), 1);
    }

    #[test]
    fn allows_indexof_gt_zero() {
        // #3840: `> 0` is a valid "present at a non-leading position" check.
        assert!(run_on("if (foo.indexOf(x) > 0) {}").is_empty());
    }

    #[test]
    fn allows_custom_element_tag_check() {
        // #3840: a custom-element tag must contain `-` but cannot start with one,
        // so `> 0` is exactly correct here.
        assert!(run_on("const isCustom = tag.indexOf('-') > 0;").is_empty());
    }

    #[test]
    fn allows_indexof_gte_zero() {
        assert!(run_on("if (arr.indexOf(x) >= 0) {}").is_empty());
    }

    #[test]
    fn allows_indexof_neq_minus_one() {
        assert!(run_on("if (arr.indexOf(x) !== -1) {}").is_empty());
    }
}
