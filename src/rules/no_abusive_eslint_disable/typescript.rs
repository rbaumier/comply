//! no-abusive-eslint-disable — TS/JS/TSX backend.
//!
//! Walks `comment` AST nodes and flags eslint-disable directives that
//! don't specify which rules to disable.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] prefilter = ["eslint-disable"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !super::is_abusive_disable(text) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Specify the rules you want to disable.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bare_disable_next_line() {
        assert_eq!(run("// eslint-disable-next-line\nconst x = 1;").len(), 1);
    }

    #[test]
    fn flags_bare_disable() {
        assert_eq!(run("/* eslint-disable */").len(), 1);
    }

    #[test]
    fn flags_bare_disable_line() {
        assert_eq!(run("const x = 1; // eslint-disable-line").len(), 1);
    }

    #[test]
    fn allows_specific_rule() {
        assert!(run("// eslint-disable-next-line no-console\nconst x = 1;").is_empty());
    }

    #[test]
    fn allows_specific_rule_in_block() {
        assert!(run("/* eslint-disable no-unused-vars */").is_empty());
    }

    #[test]
    fn allows_scoped_rule() {
        assert!(run(
            "// eslint-disable-next-line @typescript-eslint/no-explicit-any\nconst x = 1;"
        )
        .is_empty());
    }

    #[test]
    fn flags_with_description_separator() {
        assert_eq!(run("// eslint-disable-next-line -- reason\nconst x = 1;").len(), 1);
    }

    #[test]
    fn ignores_non_comment_lines() {
        assert!(run("const eslintDisable = true;").is_empty());
    }
}
