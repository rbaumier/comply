//! no-array-sort-mutation oxc backend — flag `.sort()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".sort"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "sort" {
            return;
        }

        // Only fire when the receiver is a reference some other code might also
        // hold (an identifier or a member access). A receiver that is itself a
        // fresh array produced inline — an array literal or a call result such
        // as `Object.keys(o).sort()` or `items.filter(p).sort()` — has no
        // aliasing risk: the in-place mutation is not observable.
        if is_fresh_array(&member.object) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `.toSorted()` instead of `.sort()` — `sort()` mutates the array in place."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `expr` evaluates to a freshly allocated array with no
/// pre-existing reference: an array literal or a call expression result.
fn is_fresh_array(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::ArrayExpression(_) | Expression::CallExpression(_)
    )
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
mod oxc_tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_sort_on_identifier() {
        assert_eq!(run("const sorted = arr.sort();").len(), 1);
    }

    #[test]
    fn flags_sort_on_member_access() {
        assert_eq!(run("this.items.sort();").len(), 1);
    }

    #[test]
    fn skips_sort_on_object_keys_issue_482() {
        let src = "expect(Object.keys(option).sort()).toEqual(['a', 'b']);";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_sort_on_array_literal() {
        assert!(run("const s = [3, 1, 2].sort();").is_empty());
    }

    #[test]
    fn skips_sort_on_filter_result() {
        assert!(run("const s = items.filter((x) => x).sort();").is_empty());
    }
}
