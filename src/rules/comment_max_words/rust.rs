use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return };
    let body = super::strip_markers(raw);
    if !super::has_long_sentence(&body) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Comment sentence exceeds {} words. Split it — one idea per sentence.",
            super::MAX_WORDS_PER_SENTENCE
        ),
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
    fn flags_long_sentence_rust() {
        let src = "// this comment goes on and on and on and on and on and on and on and on and on and on forever and ever and never stops\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentence_rust() {
        let src = "// short note\nfn f() {}";
        assert!(run(src).is_empty());
    }
}
