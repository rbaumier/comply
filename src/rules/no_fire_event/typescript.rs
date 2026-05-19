//! no-fire-event backend — prefer `userEvent.click` over `fireEvent.click` in tests.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

crate::ast_check! { on ["call_expression"] prefilter = ["fireEvent"] => |node, source, ctx, diagnostics|
    // Only flag actual invocations: fireEvent.click(...)
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(object) = callee.child_by_field_name("object") else { return };
    let Some(property) = callee.child_by_field_name("property") else { return };
    let Ok(object_text) = object.utf8_text(source) else { return };
    if object_text != "fireEvent" {
        return;
    }
    let Ok(property_text) = property.utf8_text(source) else { return };
    if property_text != "click" {
        return;
    }
    // Only flag in test files
    let path_str = ctx.path.to_string_lossy();
    if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
        return;
    }
    let pos = callee.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-fire-event".into(),
        message: "Prefer `userEvent.click` over `fireEvent.click` — `fireEvent.click` dispatches a single synthetic click and skips the pointer/focus events a real browser fires.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(path: &str, source: &str) -> Vec<Diagnostic> {
        let check = Check;
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(Path::new(path), source);
        <Check as crate::rules::backend::AstCheck>::check(&check, &ctx, &tree)
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
