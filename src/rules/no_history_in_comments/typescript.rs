use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
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
    fn run(s: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(s, &Check) }

    #[test]
    fn flags_previously() {
        assert_eq!(run("// previously used a Map here").len(), 1);
    }

    #[test]
    fn flags_rewritten() {
        assert_eq!(run("// rewritten in v3").len(), 1);
    }

    #[test]
    fn allows_neutral_comment() {
        assert!(run("// caches results for 5 minutes").is_empty());
    }
}
