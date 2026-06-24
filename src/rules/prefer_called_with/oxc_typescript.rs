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

/// Walks the receiver chain of the assertion (`expect(mock).resolves.not`…) down
/// to the `expect(<arg>)` call and returns the name of `<arg>` when it is a bare
/// identifier. Used to detect the canonical "args inspected via a finder" pattern
/// below; member-expression or computed receivers (`expect(obj.fn)`) yield `None`.
fn expect_receiver_name<'a>(mut expr: &'a Expression<'a>) -> Option<&'a str> {
    loop {
        match expr {
            Expression::StaticMemberExpression(m) => expr = &m.object,
            Expression::CallExpression(call) => {
                let Expression::Identifier(callee) = &call.callee else {
                    return None;
                };
                if callee.name != "expect" {
                    return None;
                }
                let Expression::Identifier(arg) = call.arguments.first()?.as_expression()? else {
                    return None;
                };
                return Some(arg.name.as_str());
            }
            _ => return None,
        }
    }
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
        // Skip when the same mock's recorded arguments are inspected elsewhere via
        // `<mock>.mock.calls`. There the bare `toHaveBeenCalled()` only confirms a
        // call happened and the args ARE asserted — by resolving them through a
        // finder first, because the call carries an un-equalable argument (a
        // closure / functional reducer) that `toHaveBeenCalledWith(...)` cannot
        // deep-equal. Pushing `toHaveBeenCalledWith` there is the false positive of
        // rbaumier/comply#2259.
        //
        // The guard is deliberately broader than the issue's "in the same test
        // block" wording: it is a file-scoped substring check, so it suppresses
        // whenever ANY `<mock>.mock.calls` access exists anywhere in the file — a
        // finder, but also a plain `.mock.calls.length` or `.mock.calls[0]`, and
        // even one sitting in an unrelated `it()` block. We accept that
        // over-suppression: the rule is only a Warning, a file that reads
        // `<mock>.mock.calls` at all is asserting that mock's recorded args
        // somewhere, and the AST scope-walk a per-block check would need is not
        // worth it for the residual cross-block, same-mock-name collision. The
        // needle stays mock-specific, so a different mock's `.mock.calls` does not
        // excuse a bare assertion on this one (see the dedicated test).
        if let Some(mock) = expect_receiver_name(&member.object)
            && ctx.source_contains(&format!("{mock}.mock.calls"))
        {
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

    #[test]
    fn skips_when_args_inspected_via_mock_calls_finder() {
        // rbaumier/comply#2259: the bare `toHaveBeenCalled()` confirms a call
        // happened and the specific arg is asserted below via the finder, because
        // navigate's `search` arg is an un-equalable functional reducer that
        // `toHaveBeenCalledWith(...)` cannot deep-equal.
        let src = "expect(navigateMock).toHaveBeenCalled();\n\
            const navArg = navigateMock.mock.calls.map((c) => c.at(0)).find((a) => hasSearchObject(a));\n\
            expect(navArg).toBeDefined();";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(d.is_empty(), "args inspected via .mock.calls should not be flagged");
    }

    #[test]
    fn still_flags_when_no_subsequent_mock_calls_inspection() {
        // True positive preserved: a call carrying assertable args with no
        // `.mock.calls` finder following should still push toHaveBeenCalledWith.
        let src = "expect(mock).toHaveBeenCalled();\nexpect(mock).toReturn();";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn mock_calls_of_a_different_mock_does_not_suppress() {
        // The finder must target the SAME mock; inspecting another mock's calls
        // does not excuse a bare assertion on this one.
        let src = "expect(mock).toHaveBeenCalled();\nconst x = other.mock.calls.length;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn skips_resolves_to_have_been_called_with_mock_calls_inspection() {
        // `.resolves` (without `.not`) still reaches the suppression: the receiver
        // walk descends the `expect(mock).resolves` member spine to the
        // `expect(mock)` call and recovers the mock name.
        let src = "expect(spy).resolves.toHaveBeenCalled();\nconst c = spy.mock.calls.at(0);";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(d.is_empty(), "resolves chain should reach the .mock.calls suppression");
    }

    #[test]
    fn still_flags_member_receiver_despite_mock_calls_in_source() {
        // `expect(obj.fn)` is a member-expression receiver, not a bare identifier,
        // so the mock name cannot be recovered and the suppression never applies —
        // even though `obj.fn.mock.calls` appears in the file. The bare assertion
        // is still flagged.
        let src = "expect(obj.fn).toHaveBeenCalled();\nconst c = obj.fn.mock.calls.at(0);";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert_eq!(d.len(), 1, "member-expression receiver must not be suppressed");
    }

    #[test]
    fn over_suppresses_same_mock_name_across_unrelated_blocks() {
        // Documents the accepted over-suppression: the guard is file-scoped, so a
        // bare assertion in one block is suppressed by a `.mock.calls` access in an
        // unrelated block as long as the mock name matches. This is the deliberate
        // cross-block trade-off described in the suppression comment, pinned here
        // so a future precision change (per-block scoping) flips this expectation
        // on purpose rather than silently.
        let src = "it('a', () => { expect(mock).toHaveBeenCalled(); });\n\
            it('b', () => { const c = mock.mock.calls.at(0); });";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.ts");
        assert!(d.is_empty(), "file-scoped guard suppresses across blocks for the same mock name");
    }
}
