use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
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
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_long_sentence() {
        let src = "// this comment goes on and on and on and on and on and on and on and on and on and on forever please stop right now";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentence() {
        assert!(run("// short and sweet").is_empty());
    }

    #[test]
    fn allows_two_short_sentences() {
        assert!(run("// first thing happens. second thing happens.").is_empty());
    }
}
