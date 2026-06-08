//! block-scope-case backend — flag `case` clauses with unwrapped lexical
//! declarations.
//!
//! A `switch_case` in tree-sitter's TS/JS grammar contains the keyword
//! `case`, the `value`, and then the body statements as siblings. When
//! the body contains a `lexical_declaration` (`let`/`const`) or
//! `class_declaration` that is NOT wrapped in a `statement_block`, the
//! binding leaks into adjacent cases and can trigger TDZ errors.
//!
//! The fix is to wrap the case body in `{ ... }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["switch_case", "switch_default"] => |node, source, ctx, diagnostics|
    let _ = source;
    // Iterate direct named children; skip the `value` (for switch_case).
    // Any direct `lexical_declaration` / `class_declaration` is a leak.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // Skip the case label expression.
        if node.child_by_field_name("value").is_some_and(|v| v.id() == child.id()) {
            continue;
        }
        if matches!(child.kind(), "lexical_declaration" | "class_declaration") {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "block-scope-case".into(),
                message: "Lexical declaration in `case` clause leaks into sibling cases — wrap the body in `{ ... }`.".into(),
                severity: Severity::Warning,
                span: Some((child.byte_range().start, child.byte_range().len())),
            });
            // One diagnostic per case is enough; stop after the first.
            return;
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_const_in_case_without_block() {
        let src = r#"switch (x) {
    case 1:
        const y = 2;
        break;
    case 2:
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_let_in_case_without_block() {
        let src = r#"switch (x) {
    case 1:
        let y = 2;
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_class_decl_in_case() {
        let src = r#"switch (x) {
    case 1:
        class Foo {}
        break;
}"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_case_with_block() {
        let src = r#"switch (x) {
    case 1: {
        const y = 2;
        break;
    }
    case 2:
        break;
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_case_without_declaration() {
        let src = r#"switch (x) {
    case 1:
        doSomething();
        break;
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_default_with_block() {
        let src = r#"switch (x) {
    case 1:
        break;
    default: {
        const y = 2;
        break;
    }
}"#;
        assert!(run_on(src).is_empty());
    }
}
