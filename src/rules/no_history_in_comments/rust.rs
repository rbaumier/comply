use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return };
    if !super::mentions_history(raw) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Comment narrates history (`was`, `previously`, `refactored`, `rewritten`). Describe current behaviour — history lives in git log.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_rust(s, &Check) }

    #[test]
    fn flags_previously_rust() {
        let src = "// previously used HashMap\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_neutral_comment_rust() {
        let src = "// caches results\nfn f() {}";
        assert!(run(src).is_empty());
    }
}
