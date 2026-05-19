use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
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
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "fireEvent" {
            return;
        }
        if member.property.name.as_str() != "click" {
            return;
        }
        let path_str = ctx.path.to_string_lossy();
        if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `userEvent.click` over `fireEvent.click` — `fireEvent.click` dispatches a single synthetic click and skips the pointer/focus events a real browser fires.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_fire_event_in_test() {
        let diags = run_on(
            "components/__tests__/button.test.tsx",
            "fireEvent.click(button)",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_user_event() {
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "userEvent.click(button)"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run_on("components/button.tsx", "fireEvent.click(button)").is_empty());
    }

    #[test]
    fn allows_fire_event_focus() {
        assert!(
            run_on(
                "components/__tests__/combobox.test.tsx",
                "fireEvent.focus(screen.getByRole(\"combobox\"))",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_blur() {
        assert!(
            run_on("components/__tests__/input.test.tsx", "fireEvent.blur(el)").is_empty()
        );
    }

    #[test]
    fn allows_fire_event_key_down() {
        assert!(
            run_on(
                "components/__tests__/input.test.tsx",
                "fireEvent.keyDown(el, { key: \"Enter\" })",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_change() {
        assert!(
            run_on(
                "components/__tests__/input.test.tsx",
                "fireEvent.change(el, { target: { value: \"x\" } })",
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_fire_event_pointer_down() {
        assert!(
            run_on(
                "components/__tests__/popover.test.tsx",
                "fireEvent.pointerDown(el)",
            )
            .is_empty()
        );
    }

    #[test]
    fn no_flag_bare_reference_in_foreach() {
        // fireEvent.click passed as a callback — not an invocation
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "array.forEach(fireEvent.click)",
            )
            .is_empty()
        );
    }

    #[test]
    fn no_flag_bare_reference_assigned() {
        // fireEvent.click assigned to a variable — not an invocation
        assert!(
            run_on(
                "components/__tests__/button.test.tsx",
                "const handler = fireEvent.click;",
            )
            .is_empty()
        );
    }
}
