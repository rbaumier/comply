//! expiring-todo-comments — Rust backend.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if !matches!(node.kind(), "line_comment" | "block_comment") { return; }
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
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_expired_todo() {
        let diags = run("// TODO [2020-01-01]: migrate to v2\nfn f() {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("2020-01-01"));
    }

    #[test]
    fn allows_future_date() {
        assert!(run("// TODO [2099-12-31]: future task\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_todo_without_date() {
        assert!(run("// TODO fix this later\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_expired_in_block_comment() {
        let diags = run("/* TODO [2019-03-01]: old task */\nfn f() {}");
        assert_eq!(diags.len(), 1);
    }
}
