use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// The `toHaveBeenCalledWith` matcher family.
const CALLED_WITH_MATCHERS: [&str; 3] = [
    "toHaveBeenCalledWith",
    "toHaveBeenLastCalledWith",
    "toHaveBeenNthCalledWith",
];

/// Walks the receiver chain of a member expression and returns true if any
/// intermediate property is `not` (e.g. `expect(x).not`, `expect(x).resolves.not`).
fn has_not_in_chain(mut expr: &Expression) -> bool {
    while let Expression::StaticMemberExpression(m) = expr {
        if m.property.name == "not" {
            return true;
        }
        expr = &m.object;
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toHaveBeenCalledWith", "toHaveBeenLastCalledWith", "toHaveBeenNthCalledWith"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if !CALLED_WITH_MATCHERS.contains(&member.property.name.as_str()) {
            return;
        }
        // Only the `.not.`-negated form is the footgun; the positive matcher is fine.
        if !has_not_in_chain(&member.object) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`not.{}(...)` passes whenever the mock was called with any other arguments, so it never fails. Use `expect(fn).not.toHaveBeenCalled()` or assert over `fn.mock.calls`.",
                member.property.name.as_str()
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
    fn flags_negated_to_have_been_called_with() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(fn).not.toHaveBeenCalledWith(payload);",
            "t.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_negated_to_have_been_last_called_with() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(fn).not.toHaveBeenLastCalledWith(x);",
            "t.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_negated_to_have_been_nth_called_with() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(fn).not.toHaveBeenNthCalledWith(1, x);",
            "t.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_negated_via_resolves_chain() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "await expect(fn).resolves.not.toHaveBeenCalledWith(x);",
            "t.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_negated_to_have_been_called() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(fn).not.toHaveBeenCalled();",
            "t.ts",
        );
        assert!(d.is_empty(), "`not.toHaveBeenCalled()` is the correct form");
    }

    #[test]
    fn allows_positive_to_have_been_called_with() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(fn).toHaveBeenCalledWith(payload);",
            "t.ts",
        );
        assert!(d.is_empty(), "the positive matcher asserts what authors expect");
    }

    #[test]
    fn allows_positive_last_and_nth() {
        let d = crate::rules::test_helpers::run_rule(
            &Check,
            "expect(fn).toHaveBeenLastCalledWith(x); expect(fn).toHaveBeenNthCalledWith(1, x);",
            "t.ts",
        );
        assert!(d.is_empty());
    }
}
