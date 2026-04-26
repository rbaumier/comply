//! expiring-todo-comments — TS/JS/TSX backend.
//!
//! Walks `comment` AST nodes and flags TODO/FIXME entries with a
//! bracketed ISO-8601 expiration date that is in the past.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    let today = super::today_u32();
    if let Some(diag_msg) = super::check_comment_text(text, today) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            diag_msg,
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_expired_todo() {
        let diags = run("// TODO [2020-01-01]: migrate to v2");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("2020-01-01"));
        assert!(diags[0].message.contains("expired"));
    }

    #[test]
    fn flags_expired_fixme() {
        let diags = run("// FIXME [2021-06-15]: remove workaround");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("FIXME"));
    }

    #[test]
    fn allows_future_date() {
        assert!(run("// TODO [2099-12-31]: future task").is_empty());
    }

    #[test]
    fn allows_todo_without_date() {
        assert!(run("// TODO fix this later").is_empty());
    }

    #[test]
    fn allows_todo_with_non_date_bracket() {
        assert!(run("// TODO [needs-review]: check this").is_empty());
    }

    #[test]
    fn flags_expired_in_block_comment() {
        let diags = run("/* TODO [2019-03-01]: old task */");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_code_not_comment() {
        assert!(run("const date = '2020-01-01';").is_empty());
    }
}
