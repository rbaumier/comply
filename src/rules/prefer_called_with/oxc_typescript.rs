use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

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
        Some(&["toHaveBeenCalled"])
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
        if member.property.name.as_str() != "toHaveBeenCalled" {
            return;
        }
        // Must have zero arguments.
        if !call.arguments.is_empty() {
            return;
        }
        // Skip negated assertions.
        if has_not_in_chain(&member.object) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `toHaveBeenCalledWith(...)` to assert specific arguments instead of bare `toHaveBeenCalled()`.".into(),
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
    fn flags_bare_to_have_been_called() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(mock).toHaveBeenCalled();", "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_have_been_called_with() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(mock).toHaveBeenCalledWith(1, 2);", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_negated_to_have_been_called() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(CAPTURE_EXCEPTION_MOCK).not.toHaveBeenCalled();", "t.ts");
        assert!(d.is_empty(), "negated assertion should not be flagged");
    }

    #[test]
    fn skips_resolves_not_to_have_been_called() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(mock).resolves.not.toHaveBeenCalled();", "t.ts");
        assert!(d.is_empty());
    }

    #[test]
    fn skips_rejects_not_to_have_been_called() {
        let d = crate::rules::test_helpers::run_rule(&Check, "expect(mock).rejects.not.toHaveBeenCalled();", "t.ts");
        assert!(d.is_empty());
    }
}
