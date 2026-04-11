//! no-fire-event backend — prefer `userEvent` over `fireEvent` in tests.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "member_expression" {
        return;
    }
    let Ok(text) = node.utf8_text(source) else { return };
    if !text.starts_with("fireEvent.") {
        return;
    }
    // Only flag in test files
    let path_str = ctx.path.to_string_lossy();
    if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-fire-event".into(),
        message: "Prefer `userEvent` over `fireEvent` — `fireEvent` dispatches a single synthetic event and skips intermediate browser events.".into(),
        severity: Severity::Warning,
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
        let diags = run_on("components/__tests__/button.test.tsx", "fireEvent.click(button)");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_user_event() {
        assert!(run_on("components/__tests__/button.test.tsx", "userEvent.click(button)").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        assert!(run_on("components/button.tsx", "fireEvent.click(button)").is_empty());
    }
}
