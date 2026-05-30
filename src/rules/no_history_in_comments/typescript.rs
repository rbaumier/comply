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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_previously_used() {
        assert_eq!(run("// previously used a Map here").len(), 1);
    }

    #[test]
    fn flags_rewritten() {
        assert_eq!(run("// was rewritten in v3").len(), 1);
    }

    #[test]
    fn allows_rewritten_as_functional_verb() {
        // "rewritten" describing runtime behaviour, not a code-change history
        assert!(run("// non-string — should not be rewritten or crash").is_empty());
    }

    #[test]
    fn flags_was_replaced() {
        assert_eq!(run("// was replaced with a Set").len(), 1);
    }

    #[test]
    fn allows_neutral_comment() {
        assert!(run("// caches results for 5 minutes").is_empty());
    }

    #[test]
    fn allows_descriptive_was() {
        assert!(run("// check if the value was provided").is_empty());
    }

    #[test]
    fn allows_jsdoc_with_was() {
        assert!(run("/** Returns whether the item was found */").is_empty());
    }
}
