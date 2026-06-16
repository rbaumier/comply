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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_previously_used() {
        assert_eq!(run("// previously used a Map here").len(), 1);
    }

    #[test]
    fn flags_rewritten() {
        assert_eq!(run("// rewritten in v3").len(), 1);
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

    #[test]
    fn allows_be_rewritten_as_behaviour() {
        // Regression for issue #494: "be rewritten" describes expected behaviour
        // (a verb), not a past code change.
        assert!(run("// non-string — should not be rewritten or crash").is_empty());
        assert!(run("// the URL will be rewritten to strip query params").is_empty());
    }

    #[test]
    fn allows_runtime_filesystem_condition() {
        // Regression for issue #3242: "was deleted" with an indefinite/domain
        // subject describes a runtime filesystem event, not code history.
        assert!(run("// some file was deleted").is_empty());
        assert!(run("// if the record was removed").is_empty());
    }

    #[test]
    fn flags_ambiguous_verb_with_code_subject() {
        // The ambiguous verb still fires when the subject is a code artifact.
        assert_eq!(run("// the validateUser function was removed").len(), 1);
    }

    #[test]
    fn flags_ambiguous_verb_with_camelcase_subject() {
        // A bare camelCase/PascalCase identifier is a code artifact even without
        // an artifact noun like "function".
        assert_eq!(run("// validateUser was removed").len(), 1);
        assert_eq!(run("// MyComponent was renamed").len(), 1);
        assert_eq!(run("// fetchData was updated").len(), 1);
    }

    #[test]
    fn flags_in_favor_of_marker() {
        assert_eq!(run("// The old cache layer was removed in favor of Redis").len(), 1);
    }

    #[test]
    fn handles_non_ascii_before_ambiguous_verb() {
        // Multi-byte chars shift lowercase byte offsets; the subject slice must
        // not panic. Indefinite subject => no diagnostic.
        assert!(run("// café — a file was deleted").is_empty());
    }
}
