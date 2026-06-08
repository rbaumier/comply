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
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn flags_bare_to_have_been_called() {
        let d = run_oxc_ts("expect(mock).toHaveBeenCalled();", &Check);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_have_been_called_with() {
        let d = run_oxc_ts("expect(mock).toHaveBeenCalledWith(1, 2);", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_negated_to_have_been_called() {
        let d = run_oxc_ts(
            "expect(CAPTURE_EXCEPTION_MOCK).not.toHaveBeenCalled();",
            &Check,
        );
        assert!(d.is_empty(), "negated assertion should not be flagged");
    }

    #[test]
    fn skips_resolves_not_to_have_been_called() {
        let d = run_oxc_ts("expect(mock).resolves.not.toHaveBeenCalled();", &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_rejects_not_to_have_been_called() {
        let d = run_oxc_ts("expect(mock).rejects.not.toHaveBeenCalled();", &Check);
        assert!(d.is_empty());
    }



    #[test]
    fn allows_unrelated_matcher() {
        let d = run_oxc_ts("expect(x).toBe(1);", &Check);
        assert!(d.is_empty());
    }


    #[test]
    fn flags_chained_expect_to_have_been_called() {
        let d = run_oxc_ts("expect(fn).toHaveBeenCalled();", &Check);
        assert_eq!(d.len(), 1);
    }
}
