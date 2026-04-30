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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
    }

    #[test]
    fn flags_previously_used() {
        let src = "// previously used HashMap\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_was_refactored() {
        let src = "// was refactored from a Vec\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_rewritten() {
        let src = "// rewritten in v3\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_neutral_comment() {
        assert!(run("// caches results\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_descriptive_was() {
        assert!(run("// whether it was the initial value\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_doc_comments() {
        assert!(run("/// Return to the previously set panic hook\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_doc_comment_with_was() {
        assert!(run("/// Track which source stream the value was received from\nfn f() {}").is_empty());
    }
}
